# Fork automation

This fork-friendly flow separates upstream sync from release publishing:

1. `Sync upstream` runs daily or manually from Actions.
2. It merges `Wangnov/Codex-App-Manager@main` into `sync/upstream-main` and opens
   a PR.
3. The normal CI runs on that PR.
4. After review and merge, push a `v*` tag to trigger `Release`.

This avoids publishing installers from unreviewed upstream changes or from a
merge conflict.

## Publish a release

```bash
git checkout main
git pull origin main
git tag v0.2.5
git push origin v0.2.5
```

The tag must start with `v`. The release workflow builds macOS and Windows,
signs updater artifacts, generates `latest.json`, and publishes a GitHub
Release.

For fork releases, CI temporarily rewrites the updater GitHub endpoint to the
current `GITHUB_REPOSITORY`, and `latest.json` uses the same repository. The
checked-in `tauri.conf.json` remains unchanged.

## Required secrets

Windows updater signing needs:

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

macOS signed/notarized release also needs:

- `APPLE_CERTIFICATE`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_SIGNING_IDENTITY`
- `KEYCHAIN_PASSWORD`
- `AC_API_KEY_ID`
- `AC_API_ISSUER_ID`
- `AC_API_KEY_BASE64`

Without Apple Developer ID and notarization secrets, the full cross-platform
release workflow cannot publish a production macOS build. Use CI/PR builds for
code validation, or adjust the release matrix intentionally for a Windows-only
fork release.
