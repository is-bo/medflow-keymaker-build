# MedFlow Key Maker — build mirror

This repository exists **only so GitHub Actions can build the MedFlow Key Maker
Android APK**. It is a build-only mirror of the private MedFlow Key Maker app.
Development happens in the private MedFlow repository; changes here are
overwritten by the next sync.

**It contains no secrets.** The license-signing key is created on the owner's
phone and never leaves it (except inside the owner's encrypted backup or as
the owner's 24 words on paper). Nothing in this repository can make a MedFlow
license key. The only secrets involved are the four APK-signing values stored
as encrypted GitHub Actions secrets (see below) — they are never in the code.

## Build the APK

1. **Actions** tab → **Key Maker Android APK (private)** → **Run workflow**
   (a push to `main` that touches the app also starts a build).
2. Wait about 20–30 minutes.
3. Open the finished run. Under **Artifacts**, download `medflow-keymaker-apk-<run number>`
   (a zip containing `medflow-keymaker.apk`). The job summary shows the APK
   SHA-256 and the signing-certificate SHA-256 — the certificate value must be
   the same on every build.

**Note — public repository:** anyone can download workflow artifacts of a
public repository. That is acceptable: the APK holds no secret and is useless
without the owner's password, authenticator and vault. Artifacts are deleted
automatically after **3 days**.

Install and usage instructions: [`apps/keymaker/README.md`](apps/keymaker/README.md).

## Signing secrets

The workflow refuses to run without these repository secrets (Settings →
Secrets and variables → Actions). How to create the keystore:
[`apps/keymaker/README.md`](apps/keymaker/README.md#one-time-create-the-signing-key-keystore-and-github-secrets).

| Secret | Value |
|---|---|
| `KEYMAKER_KEYSTORE_B64` | base64 of the release keystore (`.jks`) |
| `KEYMAKER_KEYSTORE_PASSWORD` | keystore password |
| `KEYMAKER_KEY_ALIAS` | key alias (`keymaker`) |
| `KEYMAKER_KEY_PASSWORD` | key password |

The workflow has no `pull_request` triggers, so pull requests from strangers
can never run with these secrets.

## Layout

| Path | What |
|---|---|
| `apps/keymaker/` | The Tauri 2 app (React UI, `src-tauri/`, `core/` security crate) |
| `crates/medflow-license/` | Shared license-key format and signing code (path dependency) |
| `.github/workflows/keymaker-android.yml` | APK build, signing and hardening checks |

Local checks: `pnpm install --frozen-lockfile`, `pnpm typecheck`, `pnpm test`,
`pnpm build`; `cargo test -p medflow-keymaker-core --locked` in `apps/keymaker`.

## License

Copyright © MedFlow. All rights reserved. The source is visible only so it can
be built; no licence to use, copy, modify or distribute it is granted.
