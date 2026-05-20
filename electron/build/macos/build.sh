#!/bin/bash
set -e
echo "Building openLoom for macOS..."
cd "$(dirname "$0")/../../.."
cargo build --release
cd web && npm run build && cd ..
cd electron && npx electron-builder --mac
echo "Done: dist/openLoom-*.dmg"
