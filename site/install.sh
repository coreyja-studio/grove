#!/bin/sh
# Grove installer — https://grove.coreyja.com
# Usage: curl -fsSL https://grove.coreyja.com/install.sh | sh
set -eu

REPO="coreyja-studio/grove"
INSTALL_DIR="${GROVE_INSTALL_DIR:-/usr/local/bin}"
BINARY_NAME="grove"

info() { printf '\033[1;32m%s\033[0m\n' "$*"; }
warn() { printf '\033[1;33m%s\033[0m\n' "$*" >&2; }
err() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

detect_platform() {
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux)  os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *)      err "Unsupported OS: $os" ;;
  esac

  case "$arch" in
    x86_64|amd64)  arch="x86_64" ;;
    aarch64|arm64) arch="aarch64" ;;
    *)             err "Unsupported architecture: $arch" ;;
  esac

  echo "${arch}-${os}"
}

get_latest_version() {
  url="https://api.github.com/repos/${REPO}/releases/latest"
  if command -v curl >/dev/null 2>&1; then
    version="$(curl -fsSL "$url" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')"
  elif command -v wget >/dev/null 2>&1; then
    version="$(wget -qO- "$url" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')"
  else
    err "Neither curl nor wget found. Please install one and try again."
  fi

  [ -z "$version" ] && err "Could not determine latest version"
  echo "$version"
}

download() {
  url="$1"
  dest="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$dest"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO "$dest" "$url"
  fi
}

main() {
  platform="$(detect_platform)"
  version="$(get_latest_version)"

  info "Installing grove ${version} for ${platform}..."

  archive="grove-${version}-${platform}.tar.gz"
  download_url="https://github.com/${REPO}/releases/download/${version}/${archive}"

  tmpdir="$(mktemp -d)"
  trap 'rm -rf "$tmpdir"' EXIT

  info "Downloading ${download_url}..."
  download "$download_url" "${tmpdir}/${archive}"

  info "Extracting..."
  tar -xzf "${tmpdir}/${archive}" -C "$tmpdir"

  # Find the binary — it may be at the top level or in a subdirectory
  binary="$(find "$tmpdir" -name "$BINARY_NAME" -type f | head -1)"
  [ -z "$binary" ] && err "Could not find '${BINARY_NAME}' binary in archive"
  chmod +x "$binary"

  # Install
  if [ -w "$INSTALL_DIR" ]; then
    mv "$binary" "${INSTALL_DIR}/${BINARY_NAME}"
  else
    info "Installing to ${INSTALL_DIR} (requires sudo)..."
    sudo mv "$binary" "${INSTALL_DIR}/${BINARY_NAME}"
  fi

  info "grove ${version} installed to ${INSTALL_DIR}/${BINARY_NAME}"
  info ""
  info "Run 'grove --help' to get started."
}

main
