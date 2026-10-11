#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET_DIR="${ROOT_DIR}/src-tauri/resources/python"
PY_SCRIPTS_TARGET="${ROOT_DIR}/src-tauri/resources/py-scripts"

if [ -x "${TARGET_DIR}/bin/python3" ] || [ -f "${TARGET_DIR}/python.exe" ]; then
    echo "==> Portable Python already present in ${TARGET_DIR}. Skipping download."
    mkdir -p "${PY_SCRIPTS_TARGET}"
    cp -f "${ROOT_DIR}/py-scripts/extractor_cli.py" "${PY_SCRIPTS_TARGET}/" 2>/dev/null || true
    cp -f "${ROOT_DIR}/py-scripts/yahoo_stock_extract.py" "${PY_SCRIPTS_TARGET}/" 2>/dev/null || true
    exit 0
fi

ARCH="$(uname -m)"
OS="$(uname -s)"
TAG="20241016"
VER="3.11.10"

if [ "$OS" = "Darwin" ]; then
    if [ "$ARCH" = "arm64" ]; then
        TRIPLE="aarch64-apple-darwin"
    else
        TRIPLE="x86_64-apple-darwin"
    fi
elif [ "$OS" = "Linux" ]; then
    TRIPLE="x86_64-unknown-linux-gnu"
else
    TRIPLE="x86_64-pc-windows-msvc"
fi

URL="https://github.com/astral-sh/python-build-standalone/releases/download/${TAG}/cpython-${VER}+${TAG}-${TRIPLE}-install_only.tar.gz"

echo "==> Downloading python-build-standalone (${TRIPLE})..."
mkdir -p "${ROOT_DIR}/src-tauri/resources"
rm -rf "${TARGET_DIR}"

# Tarball contains top-level python/ — extract into resources/ to create resources/python/
curl -sSL "${URL}" | tar -xz -C "${ROOT_DIR}/src-tauri/resources/"

echo "==> Staging py-scripts into resources..."
mkdir -p "${PY_SCRIPTS_TARGET}"
cp -f "${ROOT_DIR}/py-scripts/extractor_cli.py" "${PY_SCRIPTS_TARGET}/"
cp -f "${ROOT_DIR}/py-scripts/yahoo_stock_extract.py" "${PY_SCRIPTS_TARGET}/"

echo "==> Installing yfinance dependencies..."
PYTHON_BIN="${TARGET_DIR}/bin/python3"
if [ ! -f "${PYTHON_BIN}" ]; then
    PYTHON_BIN="${TARGET_DIR}/python.exe"
fi

cd "${ROOT_DIR}"
"${PYTHON_BIN}" -m pip install --upgrade pip
"${PYTHON_BIN}" -m pip install -r py-scripts/requirements.txt

echo "==> Portable Python successfully staged at ${TARGET_DIR}!"
