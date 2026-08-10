#!/usr/bin/env bash
# Notarize + staple a signed macOS .app using App Store Connect API key creds.
# Run AFTER scripts/sign-macos-app.sh — the bundle must already carry a real
# "Developer ID Application" signature with hardened runtime, or notarization
# is rejected.
#
# Credentials come from the environment (NEVER commit these — set them as CI
# secrets or local env vars):
#   AC_API_KEY_ID     App Store Connect API key id      (e.g. 2X9R4HXF34)
#   AC_API_ISSUER_ID  issuer id (UUID)
#   AC_API_KEY        path to the AuthKey_XXXXXXXX.p8 private key file
set -euo pipefail

APP="${1:-}"
[[ -n "$APP" && -d "$APP" ]] || { echo "usage: notarize-macos.sh <path-to-.app>" >&2; exit 2; }
[[ "$(uname -s)" == "Darwin" ]] || { echo "macOS only" >&2; exit 1; }
: "${AC_API_KEY_ID:?set AC_API_KEY_ID}"
: "${AC_API_KEY:?set AC_API_KEY (path to the .p8 file)}"
# AC_API_ISSUER_ID is REQUIRED for Team keys but MUST be omitted for Individual
# keys (Xcode 26+) — passing it with an Individual key returns 401. So it's
# optional here: set it for Team keys, leave it unset for Individual keys.

log() { printf '\033[35m[notarize]\033[0m %s\n' "$*"; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
ZIP="$WORK/$(basename "$APP" .app).zip"

log "zipping bundle for submission"
ditto -c -k --keepParent "$APP" "$ZIP"

ISSUER_ARG=()
[[ -n "${AC_API_ISSUER_ID:-}" ]] && ISSUER_ARG=(--issuer "$AC_API_ISSUER_ID")

log "submitting to Apple notary service (a few minutes)…"
xcrun notarytool submit "$ZIP" \
  --key "$AC_API_KEY" \
  --key-id "$AC_API_KEY_ID" \
  "${ISSUER_ARG[@]}" \
  --wait

log "stapling ticket onto the app"
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"
log "OK — notarized + stapled: $APP"
