#!/usr/bin/env bash
# bundle a release tarball: flow-<version>-<platform>-<arch>-py<python>.tgz
#
# layout:
#   flow-server, flow-cli        (binaries in root)
#   workflows/                   (bundled workflows)
#   user_nodes/                  (bundled user nodes)
#   docs/scripting/              (user-node authoring docs)
#
# usage:
#   scripts/bundle-release.sh [--platform OS] [--arch ARCH] [--python PYVER] \
#                             [--target RUST_TARGET] [--python-bin PATH] [--output-dir DIR] \
#                             [--no-python]
#
# --platform OS label, eg linux/darwin (default: host os via `uname -s`, lowercased)
# --arch     label used in the tarball name (default: host arch via `uname -m`)
# --python   python version label, eg 3.12 (default: detected from `python3 --version`)
# --target   rust target triple to build for (default: host; passed to cargo --target)
# --python-bin  path to python interpreter to link pyo3 against (default: python3)
# --no-python   build without pyo3; drops py<ver> suffix from tarball name

set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

PLATFORM=""
ARCH=""
PYVER=""
RUST_TARGET=""
PYTHON_BIN="python3"
OUT_DIR="dist"
NO_PYTHON=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --platform) PLATFORM="$2"; shift 2 ;;
        --arch) ARCH="$2"; shift 2 ;;
        --python) PYVER="$2"; shift 2 ;;
        --target) RUST_TARGET="$2"; shift 2 ;;
        --python-bin) PYTHON_BIN="$2"; shift 2 ;;
        --output-dir) OUT_DIR="$2"; shift 2 ;;
        --no-python) NO_PYTHON=1; shift ;;
        -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 2 ;;
    esac
done

VERSION="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
[[ -n "$PLATFORM" ]] || PLATFORM="$(uname -s | tr '[:upper:]' '[:lower:]')"
[[ -n "$ARCH" ]] || ARCH="$(uname -m)"
if [[ "$NO_PYTHON" -eq 0 && -z "$PYVER" ]]; then
    PYVER="$("$PYTHON_BIN" -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')"
fi

if [[ "$NO_PYTHON" -eq 1 ]]; then
    NAME="flow-${VERSION}-${PLATFORM}-${ARCH}-nopy"
else
    NAME="flow-${VERSION}-${PLATFORM}-${ARCH}-py${PYVER}"
fi
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

echo "==> building ui"
(cd ui && npm run build)

echo "==> building rust binaries (python=${PYVER:-off}, target=${RUST_TARGET:-host})"
CARGO_ARGS=(build --release --workspace)
[[ -n "$RUST_TARGET" ]] && CARGO_ARGS+=(--target "$RUST_TARGET")
if [[ "$NO_PYTHON" -eq 1 ]]; then
    CARGO_ARGS+=(--no-default-features --features typescript)
else
    export PYO3_PYTHON="$PYTHON_BIN"
fi
cargo "${CARGO_ARGS[@]}"

if [[ -n "$RUST_TARGET" ]]; then
    BIN_DIR="target/${RUST_TARGET}/release"
else
    BIN_DIR="target/release"
fi

echo "==> staging into ${STAGE}/${NAME}"
DEST="${STAGE}/${NAME}"
mkdir -p "$DEST"

install -m 0755 "${BIN_DIR}/flow-server" "${DEST}/flow-server"
install -m 0755 "${BIN_DIR}/flow-cli"    "${DEST}/flow-cli"

echo "==> generating .env.example from flow-cli env --defaults"
"${DEST}/flow-cli" env --defaults > "${DEST}/.env.example"

# copy only tracked + non-ignored files, so .state/ and other gitignored
# artifacts don't leak into the tarball
copy_clean() {
    local dir="$1"
    git ls-files -co --exclude-standard -- "$dir" | while IFS= read -r f; do
        mkdir -p "${DEST}/$(dirname "$f")"
        install -m 0644 "$f" "${DEST}/${f}"
    done
}
copy_clean workflows
copy_clean user_nodes
copy_clean docs/scripting

mkdir -p "$OUT_DIR"
TARBALL="${OUT_DIR}/${NAME}.tgz"
tar -C "$STAGE" -czf "$TARBALL" "$NAME"

echo "==> wrote ${TARBALL}"
echo "    size: $(du -h "$TARBALL" | cut -f1)"
