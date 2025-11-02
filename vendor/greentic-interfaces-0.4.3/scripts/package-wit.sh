#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${1:-${ROOT}/target/wit-packages}"
DRY_RUN=${DRY_RUN:-0}
WIT_DIR="${ROOT}/wit"

if ! command -v wasm-tools >/dev/null 2>&1; then
  echo "Error: wasm-tools not found in PATH. Install via 'cargo install wasm-tools'." >&2
  exit 1
fi

if ! command -v wkg >/dev/null 2>&1; then
  echo "Error: wkg not found in PATH. Install via 'cargo install wkg'." >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"

parse_deps() {
  local file="$1"
  local regex='(use|import)[[:space:]]+([[:alnum:]:-]+)/[[:alnum:]_.-]+@([0-9A-Za-z._-]*[0-9A-Za-z_-])'
  local deps=()
  while IFS= read -r line; do
    if [[ $line =~ $regex ]]; then
      deps+=("${BASH_REMATCH[2]}@${BASH_REMATCH[3]}")
    fi
  done < "${file}"

  if [[ ${#deps[@]} -gt 0 ]]; then
    printf '%s\n' "${deps[@]}" | sort -u
  fi
}

copy_with_deps() {
  local ref="$1"
  local dest_root="$2"

  [[ -z "${ref}" ]] && return 0

  local pkg="${ref%@*}"
  local ver="${ref##*@}"
  local src="${WIT_DIR}/${pkg//:/-}@${ver}.wit"

  if [[ ! -f "${src}" ]]; then
    echo "Missing dependency ${ref} (expected ${src})" >&2
    return 1
  fi

  local sanitized="${ref//[:@]/-}"
  local dest_dir="${dest_root}/${sanitized}"

  if [[ -d "${dest_dir}" ]]; then
    return 0
  fi

  mkdir -p "${dest_dir}"
  cp "${src}" "${dest_dir}/package.wit"

  local subdeps
  subdeps="$(parse_deps "${src}")"
  if [[ -n "${subdeps}" ]]; then
    mkdir -p "${dest_dir}/deps"
    while IFS= read -r subref; do
      [[ -z "${subref}" ]] && continue
      copy_with_deps "${subref}" "${dest_dir}/deps" || return 1
    done <<< "${subdeps}"
  fi
}

prepare_package_layout() {
  local wit_file="$1"
  local tmpdir
  tmpdir="$(mktemp -d)"
  cp "${wit_file}" "${tmpdir}/package.wit"

  local deps
  deps="$(parse_deps "${wit_file}")"
  if [[ -n "${deps}" ]]; then
    mkdir -p "${tmpdir}/deps"
    while IFS= read -r dep_ref; do
      [[ -z "${dep_ref}" ]] && continue
      if ! copy_with_deps "${dep_ref}" "${tmpdir}/deps"; then
        rm -rf "${tmpdir}"
        return 1
      fi
    done <<< "${deps}"
  fi

  echo "${tmpdir}"
}

shopt -s nullglob
wits=("${WIT_DIR}"/*.wit "${WIT_DIR}"/*/*.wit)
shopt -u nullglob

if [[ ${#wits[@]} -eq 0 ]]; then
  echo "No WIT files found under ${WIT_DIR}." >&2
  exit 0
fi

status=0

for wit_file in "${wits[@]}"; do
  package_line="$(grep -m1 '^package ' "${wit_file}" || true)"
  if [[ -z "${package_line}" ]]; then
    echo "Skipping ${wit_file}: package declaration not found" >&2
    continue
  fi

  package_ref="${package_line#package }"
  package_ref="${package_ref%;}"
  sanitized="${package_ref//[:@]/-}"
  base_name="${sanitized}"
  out_name="${base_name}.wasm"
  out_path="${OUT_DIR}/${out_name}"
  echo "Packaging ${package_ref} -> ${out_path}"

  if [[ "$(basename "${wit_file}")" == "world.wit" ]]; then
    pkg_dir="$(dirname "${wit_file}")"
    pkg_name="$(basename "${pkg_dir}")"
    if [[ "${pkg_name}" == "wasix-mcp@0.0.5" ]]; then
      echo "  Skipping packaging for upstream dependency ${package_ref}"
      continue
    fi
    if [[ "${DRY_RUN}" -eq 1 ]]; then
      echo "  (dry-run) wkg wit build --wit-dir ${pkg_dir} -o ${out_path}"
      continue
    fi
    if ! wkg wit build --wit-dir "${pkg_dir}" -o "${out_path}" >/dev/null 2>&1; then
      echo "  Failed to package ${package_ref}" >&2
      status=1
    fi
    continue
  fi

  pkg_tmp="$(prepare_package_layout "${wit_file}")" || { status=1; continue; }

  if [[ "${DRY_RUN}" -eq 1 ]]; then
    echo "  (dry-run) wasm-tools component wit --wasm ${pkg_tmp} -o ${out_path}"
    rm -rf "${pkg_tmp}"
    continue
  fi

  if ! wasm-tools component wit "${pkg_tmp}" --wasm -o "${out_path}" >/dev/null 2>&1; then
    echo "  Failed to package ${package_ref}" >&2
    status=1
  fi
  rm -rf "${pkg_tmp}"
done

echo "Artifacts written to ${OUT_DIR}"

exit "${status}"
