# Code-signing setup guide

Phase 17 polish — the certificates themselves are not in this repo
(they're per-developer / per-organisation and live in a secrets vault).
This doc captures the procedure so the day a signed release goes out,
nobody has to re-derive it from blog posts.

## macOS — Apple Developer ID signing + notarisation

### One-time setup

1. Apple Developer Program membership.
2. Create a **Developer ID Application** certificate:
   - Keychain Access → Certificate Assistant → Request a Certificate
     From a Certificate Authority…
   - Upload the `.certSigningRequest` to developer.apple.com.
   - Download the resulting `.cer`, double-click to add to the
     login keychain.
3. App-specific password for notarisation:
   - appleid.apple.com → Sign-In and Security → App-Specific Passwords →
     "aura-notarise".
4. Store credentials in CI as encrypted secrets:
   ```
   APPLE_ID                 — your Apple ID email
   APPLE_PASSWORD           — the app-specific password from (3)
   APPLE_TEAM_ID            — from developer.apple.com membership page
   APPLE_CERTIFICATE        — base64 of the .p12 export
   APPLE_CERTIFICATE_PASSWORD
   ```

### Per-release build flow

```bash
# Install the cert into the build keychain
echo "$APPLE_CERTIFICATE" | base64 -d > cert.p12
security create-keychain -p "" build.keychain
security set-keychain-settings build.keychain
security unlock-keychain -p "" build.keychain
security import cert.p12 -k build.keychain \
  -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s -k "" build.keychain

# Build + sign + notarise via tauri-cli
pnpm tauri build --target universal-apple-darwin
```

`src-tauri/tauri.conf.json`:

```jsonc
{
  "bundle": {
    "macOS": {
      "signingIdentity": "Developer ID Application: <name> (<TEAM_ID>)",
      "providerShortName": "<TEAM_ID>",
      "entitlements": "src-tauri/entitlements.plist"
    }
  }
}
```

Then:

```bash
xcrun notarytool submit out.dmg \
  --apple-id "$APPLE_ID" \
  --password "$APPLE_PASSWORD" \
  --team-id "$APPLE_TEAM_ID" \
  --wait
xcrun stapler staple out.dmg
```

## Windows — Authenticode + EV cert (recommended)

### One-time setup

1. Buy an EV code-signing certificate (DigiCert, Sectigo, etc.).
   EV is recommended over standard because:
   - SmartScreen reputation kicks in immediately, not after thousands
     of installs.
   - Stored on a hardware token (HSM) — the private key never sits on
     disk.
2. Store CI credentials:
   ```
   WINDOWS_CERTIFICATE_THUMBPRINT
   WINDOWS_SIGNING_HSM_PIN
   ```

### Per-release

```bash
# tauri.conf.json
"bundle": {
  "windows": {
    "certificateThumbprint": "<THUMBPRINT>",
    "digestAlgorithm": "sha256",
    "timestampUrl": "http://timestamp.digicert.com"
  }
}

pnpm tauri build --target x86_64-pc-windows-msvc
```

Tauri-cli signs `.msi` + `.exe` artefacts via `signtool.exe` reading
from the HSM. Timestamp URL is required so signatures stay valid after
the cert expires.

## Linux — GPG-signed `.deb` + AppImage + `.rpm`

GPG is sufficient (no commercial CA needed):

```bash
gpg --gen-key   # one-time
gpg --armor --export <KEY_ID> > public.asc

# Per-release
dpkg-sig --sign builder out.deb
rpmsign --addsign out.rpm
```

Ship `public.asc` on the download page so users can `gpg --import` and
verify before installing.

## Auto-update wiring

`tauri-plugin-updater` is configured in `tauri.conf.json`. The endpoint
points at `https://releases.aura.example/<channel>/latest.json` which
**does not exist yet** — the release-hosting bucket is a separate
infra task. Documented as next-step in
[`LANDING_PAGE_OUTLINE.md`](./LANDING_PAGE_OUTLINE.md).

Until that lives, the updater plugin is wired but inert: `plugins:
{ updater: { active: true, endpoints: [...], pubkey: "..." } }`.
First release has to be installed manually; from the second release
on, auto-update flows.

## CI sketch

```yaml
# .github/workflows/release.yml (sketch)
name: release
on:
  push:
    tags: ["v*"]
jobs:
  build:
    strategy:
      matrix:
        os: [macos-14, ubuntu-22.04, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
      - run: pnpm install --frozen-lockfile
      - run: pnpm tauri build
        env:
          APPLE_ID: ${{ secrets.APPLE_ID }}
          # ... see above
      - uses: tauri-apps/tauri-action@v0
        with:
          tagName: ${{ github.ref_name }}
          releaseName: Aura ${{ github.ref_name }}
```
