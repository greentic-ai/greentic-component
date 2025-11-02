#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_SCRIPT="${ROOT}/scripts/package-wit.sh"
DEFAULT_OUT_DIR="${ROOT}/target/wit-packages"
DEFAULT_REGISTRY="ghcr.io"
REPO_PREFIX="wit"

OUT_DIR="${DEFAULT_OUT_DIR}"
DRY_RUN=0
REGISTRY="${DEFAULT_REGISTRY}"
SKIP_PACKAGE=0

usage() {
  cat <<USAGE
Usage: $(basename "$0") [--out-dir DIR] [--registry HOST] [--dry-run] [--skip-package]

Builds & pushes all WIT packages under ./wit/ to an OCI registry (default ghcr.io).

Environment:
  GHCR_USER  Registry account / organization (required)
  GHCR_TOKEN Registry token / PAT with write:packages (required)
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out-dir) OUT_DIR="$2"; shift 2;;
    --registry) REGISTRY="$2"; shift 2;;
    --dry-run) DRY_RUN=1; shift;;
    --skip-package) SKIP_PACKAGE=1; shift;;
    -h|--help) usage; exit 0;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 1;;
  esac
done

command -v wkg >/dev/null || { echo "Install 'wkg' (cargo install wkg)"; exit 1; }
command -v docker >/dev/null || { echo "Docker CLI is required for registry login."; exit 1; }

GHCR_USER="${GHCR_USER:-}"
GHCR_TOKEN="${GHCR_TOKEN:-}"

if [[ -z "$GHCR_USER" || -z "$GHCR_TOKEN" ]]; then
  echo "Set GHCR_USER and GHCR_TOKEN environment variables before publishing." >&2
  exit 1
fi

SKIP_PACKAGE="${SKIP_PACKAGE:-0}"
if [[ "${SKIP_PACKAGE}" -eq 0 ]]; then
  if [[ "${DRY_RUN}" -eq 1 ]]; then
    echo "(dry-run) bash ${PACKAGE_SCRIPT} ${OUT_DIR}"
  else
    bash "${PACKAGE_SCRIPT}" "${OUT_DIR}"
  fi
fi

shopt -s nullglob
wits=("${ROOT}"/wit/*.wit "${ROOT}"/wit/*/*.wit)
shopt -u nullglob

[[ ${#wits[@]} -gt 0 ]] || { echo "No WIT files found under ${ROOT}/wit."; exit 0; }

mkdir -p "${OUT_DIR}"

if [[ "${DRY_RUN}" -eq 1 ]]; then
  echo "(dry-run) echo \"\$GHCR_TOKEN\" | docker login ${REGISTRY} -u \"${GHCR_USER}\" --password-stdin"
else
  echo "Logging into ${REGISTRY} as ${GHCR_USER}"
  if ! echo "${GHCR_TOKEN}" | docker login "${REGISTRY}" -u "${GHCR_USER}" --password-stdin; then
    echo "Docker login failed" >&2
    exit 1
  fi
fi

status=0
for wit_file in "${wits[@]}"; do
  pkg_line="$(grep -m1 '^package ' "${wit_file}" || true)"
  if [[ -z "$pkg_line" ]]; then
    echo "Skipping ${wit_file}: missing 'package' declaration" >&2
    status=1
    continue
  fi

  ref="${pkg_line#package }"
  ref="${ref%;}"

  name="${ref}"
  version_tag="latest"
  if [[ "$ref" == *@* ]]; then
    name="${ref%@*}"
    version_tag="${ref##*@}"
  fi

  namespace="${name%%:*}"
  package="${name##*:}"

  sanitized_ref="${ref//[:@]/-}"
  artifact="${OUT_DIR}/${sanitized_ref}.wasm"

  if [[ "${ref}" == "wasix:mcp@0.0.5" ]]; then
    echo "Skipping publish for upstream package ${ref}"
    continue
  fi

  sanitized_version="${version_tag//[:@]/-}"
  [[ -z "${sanitized_version}" ]] && sanitized_version="latest"

  if [[ "${DRY_RUN}" -eq 1 ]]; then
    echo "Preparing ${ref}"
    echo "  (dry-run) ensure artifact: ${artifact}"
  fi

  if [[ "${DRY_RUN}" -eq 0 && ! -f "${artifact}" ]]; then
    echo "  Artifact ${artifact} not found; run without --skip-package." >&2
    status=1
    continue
  fi

  image="${REGISTRY}/${GHCR_USER}/${REPO_PREFIX}/${namespace}/${package}:${sanitized_version}"

  if [[ "${DRY_RUN}" -eq 1 ]]; then
    echo "  (dry-run) wkg oci push ${image} ${artifact}"
    continue
  fi

  if ! out="$(wkg oci push "${image}" "${artifact}" 2>&1)"; then
    echo "${out}" >&2
    echo "  Failed to publish ${ref}" >&2
    status=1
  else
    printf '%s\n' "${out}"
  fi
done

exit "${status}"
