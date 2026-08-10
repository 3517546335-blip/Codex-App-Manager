#!/usr/bin/env bash
# Vendor the Sparkle `BinaryDelta` CLI into src-tauri/resources/ so macOS
# delta (incremental) updates work. The binary is NOT committed (see .gitignore);
# run this once before `tauri build` on macOS to produce a delta-capable bundle.
#
# Why pinned to Sparkle 2.9.1: it matches the Sparkle framework version embedded
# in OpenAI's Codex.app, so this tool reads OpenAI's delta patch format (v4.2)
# exactly. Sparkle ships `bin/BinaryDelta` already universal (x86_64 + arm64) and
# code-signed (adhoc + hardened runtime), so we copy it AS-IS. Do NOT run `lipo`
# on it: lipo rewrites the Mach-O and strips the embedded code signature.
#
# Supply-chain: the release tarball is checked against a pinned SHA-256 before
# extraction, and the extracted binary against a pinned SHA-256 before install.
set -euo pipefail

SPARKLE_VERSION="2.9.1"
TARBALL="Sparkle-${SPARKLE_VERSION}.tar.xz"
TARBALL_URL="https://github.com/sparkle-project/Sparkle/releases/download/${SPARKLE_VERSION}/${TARBALL}"
TARBALL_SHA256="c0dde519fd2a43ddfc6a1eb76aec284d7d888fe281414f9177de3164d98ba4c7"
BINARY_SHA256="5c31312b5dd6bbfa4d3adf79360f0851b9369a72b5facf7f7b4df0906f4fcf67"

# This script lives in <repo>/scripts/.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${ROOT}/src-tauri/resources/BinaryDelta"

log() { printf '\033[36m[vendor-binary-delta]\033[0m %s\n' "$*"; }
die() { printf '\033[31m[vendor-binary-delta] ERROR:\033[0m %s\n' "$*" >&2; exit 1; }

# macOS-only: BinaryDelta is a macOS Sparkle tool. Skip gracefully elsewhere so
# Windows/Linux builds don't fail (delta is macOS-only; the client falls back to
# a full-package update whenever this tool is absent).
if [[ "$(uname -s)" != "Darwin" ]]; then
  log "not macOS ($(uname -s)); skipping — delta updates are macOS-only."
  exit 0
fi

sha256() { shasum -a 256 "$1" | awk '{print $1}'; }

# Idempotent: skip when already vendored with the exact expected bytes.
if [[ -f "$DEST" ]] && [[ "$(sha256 "$DEST")" == "$BINARY_SHA256" ]]; then
  log "already vendored (sha256 matches): ${DEST#"$ROOT"/}"
  exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

log "downloading Sparkle ${SPARKLE_VERSION} …"
curl -fsSL "$TARBALL_URL" -o "${TMP}/${TARBALL}" || die "download failed: $TARBALL_URL"

got="$(sha256 "${TMP}/${TARBALL}")"
[[ "$got" == "$TARBALL_SHA256" ]] || die "tarball sha256 mismatch (got $got, want $TARBALL_SHA256)"
log "tarball sha256 verified."

log "extracting bin/BinaryDelta …"
tar -xJf "${TMP}/${TARBALL}" -C "$TMP" bin/BinaryDelta || die "extract failed — no bin/BinaryDelta in tarball?"

got="$(sha256 "${TMP}/bin/BinaryDelta")"
[[ "$got" == "$BINARY_SHA256" ]] || die "BinaryDelta sha256 mismatch (got $got, want $BINARY_SHA256)"

# Must stay universal. (Reading arches also confirms it's a valid fat Mach-O.)
arches="$(lipo -archs "${TMP}/bin/BinaryDelta" 2>/dev/null || true)"
[[ "$arches" == *x86_64* && "$arches" == *arm64* ]] || die "not universal (arches: ${arches:-none})"

mkdir -p "$(dirname "$DEST")"
cp "${TMP}/bin/BinaryDelta" "$DEST"
chmod +x "$DEST"

# Sanity: it must still carry a valid signature (we copied, never rewrote it).
codesign -v "$DEST" 2>/dev/null \
  || log "warning: codesign verify failed — it will be re-signed at app-signing time."

log "vendored universal BinaryDelta → ${DEST#"$ROOT"/}"
log "arches: ${arches} | sha256: ${BINARY_SHA256}"
