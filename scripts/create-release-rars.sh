#!/usr/bin/env bash
set -euo pipefail

dist="${1:-dist}"
site="${2:-S1API}"
tag="${3:-}"

if [[ -z "$tag" || "$tag" != v* ]]; then
  echo "usage: $0 <dist-dir> <site> <v-tag>" >&2
  exit 2
fi

rar_version="7.23"
rar_version_compact="${rar_version//./}"
rar_sha256="759b4b6aa0d9f77131882162951193f3a0e54bf60e1d8dc4255aa308accab588"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

rar_bin="${RAR_BIN:-}"
if [[ -z "$rar_bin" ]]; then
  rar_bin="$(command -v rar || true)"
fi

if [[ -z "$rar_bin" ]]; then
  archive="$work/rarlinux-x64-${rar_version_compact}.tar.gz"
  urls=(
    "https://www.win-rar.com/fileadmin/winrar-versions/rarlinux-x64-${rar_version_compact}.tar.gz"
    "https://www.rarlab.com/rar/rarlinux-x64-${rar_version_compact}.tar.gz"
  )
  downloaded=0
  for url in "${urls[@]}"; do
    if curl --fail --location --retry 5 --retry-all-errors --connect-timeout 20 \
      --output "$archive" "$url"; then
      downloaded=1
      break
    fi
  done
  if [[ "$downloaded" -ne 1 ]]; then
    echo "failed to download the official RAR command-line tool" >&2
    exit 1
  fi
  echo "$rar_sha256  $archive" | sha256sum --check --status
  tar -xzf "$archive" -C "$work"
  rar_bin="$work/rar/rar"
fi

if [[ ! -x "$rar_bin" ]]; then
  echo "RAR command is not executable: $rar_bin" >&2
  exit 1
fi

package() {
  local platform="$1"
  local source_name="$2"
  local source="$dist/$source_name"
  local output="$dist/CodexDesktopManager_${site}_${tag}_${platform}.rar"

  if [[ ! -f "$source" ]]; then
    echo "missing installer for RAR package: $source" >&2
    exit 1
  fi

  rm -f "$output"
  # Installers are already compressed, so store mode avoids wasting CI time.
  "$rar_bin" a -idq -m0 -ep1 "$output" "$source"
  "$rar_bin" t -idq "$output"
  echo "created $(basename "$output") containing $source_name"
}

package "Windows_x64" \
  "CodexDesktopManager_${site}_${tag}_Windows_x64-setup.exe"
package "macOS_aarch64" \
  "CodexDesktopManager_${site}_${tag}_macOS_aarch64.dmg"
package "macOS_x86_64" \
  "CodexDesktopManager_${site}_${tag}_macOS_x86_64.dmg"

