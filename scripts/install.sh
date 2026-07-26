#!/usr/bin/env bash
# install.sh — install / update / uninstall jj-plan + jj-ask + jj-status from GitHub Releases.
# One action covers all binaries; per-binary mode is not supported.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/yigegongjiang/jj-plan/main/scripts/install.sh | bash
#   curl -fsSL https://raw.githubusercontent.com/yigegongjiang/jj-plan/main/scripts/install.sh | VERSION=v0.8.23 bash
#   INSTALL_DIR=/usr/local/bin ./scripts/install.sh
#   ./scripts/install.sh uninstall

set -euo pipefail

REPO="${REPO:-yigegongjiang/jj-plan}"
VERSION="${VERSION:-latest}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
BINARIES=(jj-plan jj-ask jj-status)

err()  { printf 'install.sh: %s\n' "$*" >&2; exit 1; }
info() { printf '%s\n' "$*"; }

usage() {
  cat <<'EOF'
usage: install.sh [install|update|uninstall]
  install/update: download jj-plan + jj-ask + jj-status
  uninstall:      remove jj-plan + jj-ask + jj-status (config kept)
env:
  REPO=<owner>/<repo>   default yigegongjiang/jj-plan
  VERSION=latest|vX.Y.Z default latest
  INSTALL_DIR=<path>    default $HOME/.local/bin
EOF
}

command -v curl >/dev/null 2>&1 || err "curl is required"

case "$(uname -s)" in
  Darwin) ;;
  *) err "unsupported OS: $(uname -s) (only macOS is supported)" ;;
esac
case "$(uname -m)" in
  x86_64|amd64)  host_arch="x64" ;;
  aarch64|arm64) host_arch="arm64" ;;
  *) err "unsupported macOS architecture: $(uname -m)" ;;
esac

if [ "$VERSION" = "latest" ]; then
  base="https://github.com/${REPO}/releases/latest/download"
else
  base="https://github.com/${REPO}/releases/download/${VERSION}"
fi
checksums_url="${base}/checksums.txt"

# Fetch checksums.txt once (best-effort). Empty when missing → checksum step skipped.
checksums="$(curl -fsSL --retry 3 "$checksums_url" 2>/dev/null || true)"

verify_checksum() {
  local file="$1" asset="$2" line expected actual
  [ -n "$checksums" ] || return 0
  line="$(printf '%s\n' "$checksums" | grep " ${asset}$" || true)"
  [ -n "$line" ] || return 0
  expected="${line%% *}"
  actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  [ "$expected" = "$actual" ] || err "checksum mismatch for ${asset} (expected ${expected}, got ${actual})"
  info "    checksum OK"
}

install_one() {
  local name="$1"
  local asset="${name}-macos-${host_arch}"
  local url="${base}/${asset}"
  local tmp dest

  tmp="$(mktemp "${TMPDIR:-/tmp}/${name}.XXXXXX")"
  trap 'rm -f "$tmp"' EXIT

  info "==> downloading ${url}"
  curl -fL --retry 3 --progress-bar "$url" -o "$tmp" || err "download failed: $url"
  verify_checksum "$tmp" "$asset"
  chmod +x "$tmp"

  dest="${INSTALL_DIR}/${name}"
  mv -f "$tmp" "$dest"
  trap - EXIT

  info "installed: $dest"
  "$dest" --version || true
}

uninstall_one() {
  local name="$1"
  local dest="${INSTALL_DIR}/${name}"
  if [ -e "$dest" ]; then
    rm -f "$dest"
    info "removed: $dest"
  else
    info "not installed: $dest"
  fi
}

ACTION="${1:-install}"
if [ "$#" -gt 1 ]; then
  err "too many arguments"
fi

case "$ACTION" in
  install|update|--install|--update) ACTION="install" ;;
  uninstall|--uninstall) ACTION="uninstall" ;;
  help|-h|--help) usage; exit 0 ;;
  *) usage >&2; err "unknown action '$ACTION'" ;;
esac

if [ "$ACTION" = "uninstall" ]; then
  for n in "${BINARIES[@]}"; do uninstall_one "$n"; done
  info "config kept: ${XDG_CONFIG_HOME:-${HOME}/.config}/jj-plan/config.json (legacy ${XDG_CONFIG_HOME:-${HOME}/.config}/jjplan + ${HOME}/.jjplan still honoured)"
  exit 0
fi

mkdir -p "$INSTALL_DIR"

info "==> installing jj-plan + jj-ask + jj-status"
info "    repo:    ${REPO}"
info "    version: ${VERSION}"
info "    arch:    darwin-${host_arch}"
info "    target:  ${INSTALL_DIR}"

for n in "${BINARIES[@]}"; do install_one "$n"; done

case ":$PATH:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    info ""
    info "warning: ${INSTALL_DIR} is not in your PATH."
    info "  add this to ~/.zshrc or ~/.bashrc:"
    info "    export PATH=\"${INSTALL_DIR}:\$PATH\""
    ;;
esac
