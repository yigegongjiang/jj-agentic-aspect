#!/usr/bin/env bash
# install-local.sh — build the CLI from source and install the freshly-built
# release binaries into $HOME/.local/bin. For local dev + the release workflow's
# machine self-install; the published network installer is scripts/install.sh.
#
# Usage:
#   ./scripts/install-local.sh
#   INSTALL_DIR=/usr/local/bin ./scripts/install-local.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
BINARIES=(jj-agentic-aspect)

err()  { printf 'install-local.sh: %s\n' "$*" >&2; exit 1; }
info() { printf '%s\n' "$*"; }

case "$(uname -s)" in
  Darwin) ;;
  *) err "unsupported OS: $(uname -s) (only macOS is supported)" ;;
esac
command -v cargo >/dev/null 2>&1 || err "cargo is required (install Rust: https://rustup.rs)"

info "==> building release binaries (native arch)"
( cd "$ROOT/cli" && cargo build --release )

mkdir -p "$INSTALL_DIR"
for name in "${BINARIES[@]}"; do
  src="$ROOT/cli/target/release/$name"
  [ -x "$src" ] || err "missing build artifact: $src"
  install -m 0755 "$src" "$INSTALL_DIR/$name"
  info "installed: $INSTALL_DIR/$name ($("$INSTALL_DIR/$name" --version))"
done

case ":$PATH:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    info ""
    info "warning: ${INSTALL_DIR} is not in your PATH."
    info "  add this to ~/.zshrc or ~/.bashrc:"
    info "    export PATH=\"${INSTALL_DIR}:\$PATH\""
    ;;
esac
