#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IDE_BIN="/Applications/Antigravity IDE.app/Contents/Resources/app/bin/antigravity-ide"

echo "==> Building Rust release binary..."
cargo build --release

echo "==> Building Antigravity IDE extension..."
cd "$SCRIPT_DIR/extension"
npm run build
npm run package

VSIX_FILE="$SCRIPT_DIR/extension/antigravity-subagent-0.1.0.vsix"

if [ -f "$IDE_BIN" ]; then
    echo "==> Installing extension into Antigravity IDE..."
    "$IDE_BIN" --install-extension "$VSIX_FILE" --force
    echo "==> Extension successfully installed into Antigravity IDE!"
    echo "==> Registered extensions:"
    "$IDE_BIN" --list-extensions --show-versions | grep -i subagent || true
else
    echo "Antigravity IDE CLI not found at $IDE_BIN"
    echo "Manual installation: Install '$VSIX_FILE' via Antigravity IDE Extensions menu."
fi
