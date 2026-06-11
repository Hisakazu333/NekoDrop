#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_TESTS=1
COPY_BUNDLES=1
BUNDLES="app"
BUILD_DMG=0
STAMP="$(date +%Y%m%d-%H%M%S)"

usage() {
  cat <<'USAGE'
NekoDrop desktop packaging

Usage:
  bash scripts/package-desktop.sh [--skip-tests] [--no-copy] [--dmg]

Options:
  --skip-tests   Build without running cargo test.
  --no-copy      Leave Tauri bundles under the package target directory only.
  --dmg          Build a signed app bundle, then create and verify a DMG.
  -h, --help     Show this help.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-tests)
      RUN_TESTS=0
      shift
      ;;
    --no-copy)
      COPY_BUNDLES=0
      shift
      ;;
    --dmg)
      BUNDLES="app"
      BUILD_DMG=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 2
      ;;
  esac
done

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

if [[ -d /opt/homebrew/opt/rustup/bin ]]; then
  export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi

require_command npm
require_command cargo

cd "$ROOT_DIR"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target/package-desktop/$STAMP}"

if [[ -z "${RUSTC:-}" ]]; then
  RUSTC_PATH="$(command -v rustc || true)"
  if [[ -n "$RUSTC_PATH" ]]; then
    export RUSTC="$RUSTC_PATH"
  fi
fi

if [[ -z "${RUSTDOC:-}" ]]; then
  RUSTDOC_PATH="$(command -v rustdoc || true)"
  if [[ -n "$RUSTDOC_PATH" ]]; then
    export RUSTDOC="$RUSTDOC_PATH"
  fi
fi

echo "==> Building desktop frontend"
npm run build

if [[ "$RUN_TESTS" -eq 1 ]]; then
  echo "==> Running Rust workspace tests"
  cargo test --workspace
fi

echo "==> Building Tauri desktop bundle"
npm --workspace apps/desktop run tauri -- build --bundles "$BUNDLES"

APP_BUNDLE="$CARGO_TARGET_DIR/release/bundle/macos/NekoDrop.app"

if [[ -d "$APP_BUNDLE" ]]; then
  echo "==> Signing macOS app bundle"
  codesign --force --deep --sign - "$APP_BUNDLE"
  codesign --verify --deep --strict "$APP_BUNDLE"
fi

if [[ "$BUILD_DMG" -eq 1 ]]; then
  if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "Missing app bundle: $APP_BUNDLE" >&2
    exit 1
  fi

  echo "==> Building macOS DMG"
  DMG_STAGING="$CARGO_TARGET_DIR/dmg-staging"
  DMG_DIR="$CARGO_TARGET_DIR/release/bundle/dmg"
  DMG_PATH="$DMG_DIR/NekoDrop_0.1.0_aarch64.dmg"

  rm -rf "$DMG_STAGING"
  mkdir -p "$DMG_STAGING" "$DMG_DIR"
  cp -R "$APP_BUNDLE" "$DMG_STAGING/NekoDrop.app"
  ln -s /Applications "$DMG_STAGING/Applications"
  hdiutil create -volname "NekoDrop" -srcfolder "$DMG_STAGING" -ov -format UDZO "$DMG_PATH"
  hdiutil verify "$DMG_PATH"
fi

if [[ "$COPY_BUNDLES" -eq 1 ]]; then
  OUTPUT_DIR="$ROOT_DIR/release/desktop/$STAMP"
  BUNDLE_DIR="$CARGO_TARGET_DIR/release/bundle"
  BINARY_PATH="$CARGO_TARGET_DIR/release/nekodrop-desktop"

  mkdir -p "$OUTPUT_DIR"

  if [[ -d "$BUNDLE_DIR" ]]; then
    cp -R "$BUNDLE_DIR" "$OUTPUT_DIR/"
  fi

  if [[ -f "$BINARY_PATH" ]]; then
    cp "$BINARY_PATH" "$OUTPUT_DIR/"
  fi

  echo "==> Package output"
  echo "$OUTPUT_DIR"
else
  echo "==> Bundle output"
  echo "$CARGO_TARGET_DIR/release/bundle"
fi
