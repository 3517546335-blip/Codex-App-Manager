#!/usr/bin/env bash
set -euo pipefail

blocked_paths=(
  "scripts/vendor-node-windows.mjs"
  "src-tauri/resources/codex-zh-CN"
)

for path in "${blocked_paths[@]}"; do
  if [ -e "$path" ]; then
    echo "::error::Legacy Chinese localization dependency must stay removed: $path" >&2
    exit 1
  fi
done

required_paths=(
  "crates/codex-win-engine/src/chinese_patch.rs"
  "crates/codex-win-engine/resources/menu-translations-zh-CN.json"
  "crates/codex-win-engine/resources/native-strings-zh-CN.json"
  "THIRD_PARTY_NOTICES.md"
)

for path in "${required_paths[@]}"; do
  if [ ! -f "$path" ]; then
    echo "::error::Native offline Chinese localization file is missing: $path" >&2
    exit 1
  fi
done

if grep -RInE \
  "npx( --yes)? @electron/asar|vendor-node-windows|下载.*(中文|语言包)|(中文|语言包).*下载" \
  src src-tauri crates .github scripts \
  --exclude="$(basename "$0")" \
  --exclude-dir=target; then
  echo "::error::Windows Chinese localization must remain native and offline; Node/npx or downloaded language-pack integration was found." >&2
  exit 1
fi

required_markers=(
  "__codexChineseMenuPatchV2"
  "__codexForceI18nV1"
  "native-menu-locales/zh-CN.json"
)

for marker in "${required_markers[@]}"; do
  if ! grep -Fq "$marker" crates/codex-win-engine/src/chinese_patch.rs; then
    echo "::error::Native offline Chinese localization marker is missing: $marker" >&2
    exit 1
  fi
done

if ! grep -Fq -- '--lang=zh-CN' crates/codex-win-engine/src/sys.rs; then
  echo "::error::The patched Windows runtime must launch with --lang=zh-CN." >&2
  exit 1
fi

echo "Windows Chinese localization is embedded, native, offline, and launch-enabled."
