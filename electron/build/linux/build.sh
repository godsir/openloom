#!/bin/bash
set -e
echo "Building openLoom for Linux..."
cd "$(dirname "$0")/../../.."
cargo build --release
cd web && npm run build && cd ..
cd electron && npx electron-builder --linux
echo "Done: dist/openLoom-*.AppImage"
