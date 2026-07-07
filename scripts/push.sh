#!/usr/bin/env bash
# push.sh — 自动统一版本号 + bump patch + push
# 用法: bash scripts/push.sh [git push 的额外参数]
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
CARGO_TOML="$ROOT_DIR/Cargo.toml"
PKG_JSON="$ROOT_DIR/frontend/package.json"

# --- 读取当前版本 ---
cargo_ver=$(grep -m1 '^version = "' "$CARGO_TOML" | sed 's/version = "\(.*\)"/\1/')
pkg_ver=$(grep -m1 '"version":' "$PKG_JSON" | sed 's/.*"version": "\(.*\)".*/\1/')

echo "当前版本: Cargo.toml=$cargo_ver  package.json=$pkg_ver"

# --- 取较高的 patch 版本作为基准，再 +1 ---
# 取两个版本中 patch 较大的
get_patch() { echo "${1##*.}"; }
cargo_patch=$(get_patch "$cargo_ver")
pkg_patch=$(get_patch "$pkg_ver")

if [ "$cargo_patch" -ge "$pkg_patch" ]; then
    base="$cargo_ver"
else
    base="$pkg_ver"
fi

# 计算新版本: patch +1
IFS='.' read -r major minor patch <<< "$base"
new_ver="${major}.${minor}.$((patch + 1))"

echo "统一后新版本: $new_ver"

# --- 写入 Cargo.toml ---
sed -i "s/version = \"$cargo_ver\"/version = \"$new_ver\"/" "$CARGO_TOML"
echo "  Cargo.toml: $cargo_ver -> $new_ver"

# --- 写入 frontend/package.json ---
sed -i "s/\"version\": \"$pkg_ver\"/\"version\": \"$new_ver\"/" "$PKG_JSON"
echo "  package.json: $pkg_ver -> $new_ver"

# --- 提交版本变更 ---
cd "$ROOT_DIR"
git add Cargo.toml frontend/package.json
git commit -m "chore: bump version to $new_ver"

echo ""
echo "版本已统一为 $new_ver 并提交，正在 push..."
echo ""

# --- Push ---
git push "$@"
