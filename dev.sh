#!/usr/bin/env bash
# Launch glyphicTTmerge in dev mode with hot-reload.
# Usage: ./dev.sh
set -euo pipefail

# Always cd via the colon-free symlink. The repo's actual path contains ':' which
# breaks both PATH (colon-separated) and Cargo's DYLD_FALLBACK_LIBRARY_PATH on macOS.
CLEAN_PATH="/Users/tea/Code/ClaudeCode/glyphicTTmerge"
if [ ! -e "$CLEAN_PATH" ]; then
  echo "error: expected symlink at $CLEAN_PATH (clean path) is missing." >&2
  echo "       create it with:  ln -s '$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)' '$CLEAN_PATH'" >&2
  exit 1
fi
cd "$CLEAN_PATH"

# Sibling of the symlinked repo, so this path is colon-clean.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(cd .. && pwd)/glyphicttmerge-target}"

# Make local node_modules binaries (vite, tauri, svelte-check) findable from any
# nested subshell that Tauri spawns for beforeDevCommand.
export PATH="$PWD/node_modules/.bin:$HOME/.bun/bin:$PATH"

if ! command -v bun >/dev/null 2>&1; then
  echo "error: bun not found on PATH. Install with: curl -fsSL https://bun.sh/install | bash" >&2
  exit 1
fi
if [ ! -d node_modules ]; then
  echo "node_modules missing — running 'bun install --ignore-scripts'..."
  bun install --ignore-scripts
fi

exec tauri dev "$@"
