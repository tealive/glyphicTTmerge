#!/usr/bin/env bash
# Produce a release build (.dmg/.app on macOS) for glyphicTTmerge.
# Usage: ./build.sh           — full bundle
#        ./build.sh --debug   — debug profile, faster
set -euo pipefail

CLEAN_PATH="/Users/tea/Code/ClaudeCode/glyphicTTmerge"
if [ ! -e "$CLEAN_PATH" ]; then
  echo "error: expected symlink at $CLEAN_PATH (clean path) is missing." >&2
  echo "       create it with:  ln -s '$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)' '$CLEAN_PATH'" >&2
  exit 1
fi
cd "$CLEAN_PATH"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(cd .. && pwd)/glyphicttmerge-target}"
export PATH="$PWD/node_modules/.bin:$HOME/.bun/bin:$PATH"

if ! command -v bun >/dev/null 2>&1; then
  echo "error: bun not found on PATH. Install with: curl -fsSL https://bun.sh/install | bash" >&2
  exit 1
fi
if [ ! -d node_modules ]; then
  echo "node_modules missing — running 'bun install --ignore-scripts'..."
  bun install --ignore-scripts
fi

exec tauri build "$@"
