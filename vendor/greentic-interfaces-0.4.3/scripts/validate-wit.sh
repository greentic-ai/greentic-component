#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WIT_DIR="${ROOT}/wit"

if ! command -v wit-bindgen >/dev/null 2>&1; then
  echo "Error: wit-bindgen not found in PATH. Install via 'cargo install wit-bindgen-cli'." >&2
  exit 1
fi

if ! command -v wasm-tools >/dev/null 2>&1; then
  echo "Error: wasm-tools not found in PATH. Install via 'cargo install wasm-tools'." >&2
  exit 1
fi

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
  echo "No WIT files found under ${WIT_DIR}."
  exit 0
fi

status=0
for wit_file in "${wits[@]}"; do
  rel_path="${wit_file#"${ROOT}/"}"
  echo "Checking ${rel_path}"

  if [[ "$(basename "${wit_file}")" == "world.wit" ]]; then
    pkg_dir="$(dirname "${wit_file}")"
    pkg_name="$(basename "${pkg_dir}")"
    case "${pkg_name}" in
      "wasix-mcp@0.0.5")
        if ! wit-bindgen markdown "${pkg_dir}" --world mcp-secrets >/dev/null 2>&1; then
          status=1
        fi
        ;;
      *)
        echo "Unknown package directory ${pkg_name}; skipping." >&2
        status=1
        ;;
    esac
    continue
  fi

  pkg_tmp="$(prepare_package_layout "${wit_file}")" || { status=1; continue; }

  out_tmp="$(mktemp -d)"
  if ! wit-bindgen markdown "${pkg_tmp}" --out-dir "${out_tmp}" >/dev/null 2>&1; then
    echo "  wit-bindgen validation failed for ${rel_path}" >&2
    status=1
  fi
  rm -rf "${out_tmp}"

  tmpwasm="$(mktemp)"
  if ! wasm-tools component wit "${pkg_tmp}" --wasm -o "${tmpwasm}" >/dev/null 2>&1; then
    echo "  wasm-tools packaging failed for ${rel_path}" >&2
    status=1
  fi
  rm -f "${tmpwasm}"
  rm -rf "${pkg_tmp}"
done

exit "${status}"
