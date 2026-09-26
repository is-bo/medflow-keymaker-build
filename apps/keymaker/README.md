# MedFlow Key Maker

Your private Android app for making MedFlow license keys. **It is for you only.**
It is never given to doctors, never put in the MedFlow installer, and never
published anywhere.

- It creates your secret **signing key** on your phone. That key never leaves
  the phone, except inside your encrypted backup file or as your 24 words.
- It turns a doctor's **machine code** (`MF-XXXX-XXXX-XXXX-XXXX`) into a
  **license key** that you send by WhatsApp or as a `.mflic` file.
- It keeps a **history** of every key you made.
- The app has **no internet access** and blocks screenshots.

---

## Guard your 24 words

> **Anyone who has your 24 words can make MedFlow keys — as many as they want,
> for any computer.** Treat them like the key to a safe.

- Write them **on paper**, in order. Never photograph them, never type them into
  another app, never send them to anyone (not even your developer).
- Keep the paper at home, away from the phone. A second copy in another safe
  place (family member, bank box) is better.
- The only times you type them are: restoring on a new phone, and the checks the
  app asks for during setup.

**If the 24 words leak** (see Settings → "If your 24 words leak"):

1. On a clean install, set up a **new** signing key (with a new name).
2. Ask your developer to ship a MedFlow update that removes the old key name.
3. Re-issue keys to your customers — the history tells you who they are.

---

## Install the app on your phone

1. Open the project on GitHub → **Actions** → **Key Maker Android APK (private)**.
2. Click **Run workflow** → **Run workflow** (green button). Wait about 20–30 minutes.
3. Open the finished run. At the bottom, under **Artifacts**, download
   `medflow-keymaker-apk-…` (a zip). It disappears after 3 days.
4. Unzip it on your phone (or send `medflow-keymaker.apk` to your phone).
5. Tap the `.apk`. Android asks to allow installing from this source — allow it
   for this one install. Tap **Install**.
6. Android may warn that the app is unknown (it is not from the Play Store).
   That is expected for a private app.

**Updating:** run the workflow again and install the new APK over the old one.
Your data stays, because every build is signed with the same key. If Android
ever says the update "conflicts with an existing package", **stop and do not
uninstall** — uninstalling deletes Key Maker's data. Make sure you have a fresh
backup first, then ask your developer.

---

## First setup (about 15 minutes, do it alone)

The app guides you step by step:

1. **Password** — at least 10 characters with letters and numbers. Nobody can
   reset it for you.
2. **24 words** — write them on paper; the app then asks for 4 of them.
3. **Authenticator** — a 6-digit code app (Google Authenticator, Microsoft
   Authenticator, Aegis). Best on a second phone (scan the QR code). On the same
   phone, tap "Open in authenticator app" or type the secret key.
4. **10 recovery codes** — each one can replace the 6-digit code once.
5. **Recovery sheet** — copy it by hand (name of the key, fingerprint, 24 words,
   10 codes).
6. **Backup file** — save it outside the phone (Google Drive, an e-mail to
   yourself, a USB stick). The app will not continue until you do.
7. **Public key** — tap Share and send that one line to your developer. It is
   safe to share. MedFlow only accepts your keys after the developer adds it.

## Everyday use

- **Open:** password + 6-digit code. After 2 wrong tries the app makes you wait
  30 seconds, then 5 minutes, then 1 hour (closing the app does not reset this).
- **It locks itself** after 2 minutes without use, and whenever you leave it.
- **New key:** doctor's name, phone (optional), machine code, duration, Telegram
  add-on → **Create key** → **Share** (pick WhatsApp) or **Save .mflic file**.
  If the machine code has a typo, the app says so before signing anything.
- **Renew:** History → tap the doctor → **Renew**. A customer with a lifetime
  key who buys Telegram needs a new **lifetime** key.
- **Backups:** when the home screen says "N keys since your last backup", open
  Backup and make a new file. The backup opens with the password you had when
  you made it.

## New phone / lost phone

Install the APK on the new phone, then choose **Restore on a new phone**:

- **Backup file** → everything comes back (key, authenticator, history).
- **24 words** → the same signing key comes back, the history starts empty, and
  you set up a new authenticator. Type the key name from your recovery sheet.

Either way, the fingerprint shown must match your recovery sheet.

**Forgot the password?** On the unlock screen: "Forgot your password?" → erase,
then restore as above.

---

## For the developer

### Layout

| Path | What |
|---|---|
| `core/` | All security logic, no Tauri: `envelope` (Argon2id → AES-256-GCM), `totp` (RFC 6238), `lockout`, `recovery`, `history`, `engine` (state machine). `cargo test -p medflow-keymaker-core` |
| `src-tauri/` | Thin Tauri 2 command layer + plugins (clipboard, dialog, fs, opener, sharekit) |
| `src/` | React + Tailwind UI (EN/FR), MedFlow design tokens |
| `scripts/patch-android.mjs` | Hardens the generated Android project (see below) |
| `../../.github/workflows/keymaker-android.yml` | Manual APK build + signing |

Key format and signing come from `crates/medflow-license` — the exact code the
desktop verifier runs. Every issued key is re-verified with it before it is shown.

### One-time: create the signing key (keystore) and GitHub secrets

The keystore signs the **APK** (so updates install over the old app). It has
nothing to do with license keys. Create it once, on a trusted computer with a
JDK (`keytool` comes with Java):

```bash
keytool -genkeypair -v -keystore keymaker-release.jks -storetype PKCS12 \
  -alias keymaker -keyalg RSA -keysize 4096 -validity 36500 \
  -dname "CN=MedFlow Key Maker, O=MedFlow, C=DZ"
# PKCS12: the key password is the keystore password.
base64 -w0 keymaker-release.jks > keymaker-release.jks.b64     # Linux / Git Bash
# PowerShell: [Convert]::ToBase64String([IO.File]::ReadAllBytes("keymaker-release.jks")) > keymaker-release.jks.b64
```

In GitHub → Settings → Secrets and variables → Actions → **New repository secret**:

| Secret | Value |
|---|---|
| `KEYMAKER_KEYSTORE_B64` | contents of `keymaker-release.jks.b64` |
| `KEYMAKER_KEYSTORE_PASSWORD` | the keystore password |
| `KEYMAKER_KEY_ALIAS` | `keymaker` |
| `KEYMAKER_KEY_PASSWORD` | the same password (PKCS12) |

Keep `keymaker-release.jks` + its password in two offline places. **Losing it
means the next APK cannot update the installed app**; the owner would have to
back up, uninstall (which deletes the vault) and restore. The workflow refuses
to run without these secrets — it never falls back to a debug key. The job
summary prints the signing-certificate SHA-256; it must be identical every time.

### Android hardening (why a patch script)

`src-tauri/gen/android` is generated by `tauri android init` from the installed
CLI's templates and needs the Android SDK, so it is not committed. CI runs
`init`, then `scripts/patch-android.mjs` (idempotent, self-checking):

- `src/release/AndroidManifest.xml` removes `INTERNET` (and network-state
  permissions) with `tools:node="remove"` — **release only**; `tauri android dev`
  still needs the network to reach the dev server.
- `FLAG_SECURE` on `MainActivity` before `super.onCreate`.
- `allowBackup="false"` + data-extraction rules excluding everything (Android 12+
  device-to-device transfer ignores `allowBackup` alone).

The workflow then checks the **signed APK** with `aapt2` (no INTERNET,
`allowBackup=false`, not debuggable).

Local Android dev (needs SDK + NDK): `pnpm tauri android init`, `pnpm android:patch`,
`pnpm tauri android dev`. Desktop dev (UI only, no share sheet guarantees):
`pnpm tauri dev` — the vault goes to `%APPDATA%\dz.medflow.keymaker`.

To ship updates: bump `version` in `src-tauri/tauri.conf.json` (Android
versionCode is derived from it and must increase).

### Security design and honest threat model

- **At rest:** one file, `keymaker.vault`, in the app's private storage:
  Argon2id (64 MiB, 3 passes, random salt) → AES-256-GCM. KDF parameters and
  salt are authenticated, so weakening them breaks decryption. Inside: seed,
  key name, TOTP secret, recovery-code hashes, history. Writes are atomic.
- **Backup** = the same data, re-encrypted with a fresh salt under the password
  typed at export time (the vault password — one password to remember; a
  separate, rarely-used backup password is the one most likely to be forgotten,
  which would make the backup useless).
- **Unlock** needs the password (it is the decryption key) **and** a TOTP code
  (±30 s, a code is never accepted twice) or a single-use recovery code.
- **Lockout** is counted before each check and saved to `lockout.json`, so
  killing the app mid-check still costs a try; restarts do not reset it.
- **In memory** the decrypted vault exists only while unlocked; it is dropped
  (and the seed zeroized, best effort) on lock, after 2 minutes idle, or when the
  app goes to the background.
- **Not done:** Android Keystore wrapping and fingerprint unlock. The official
  `tauri-plugin-biometric` only answers "fingerprint OK?" — it cannot release a
  Keystore-protected secret — so a fingerprint unlock would have required
  keeping the vault key readable on disk, which would weaken the password.

What this protects against: a lost or stolen **locked** phone (the thief needs
the password; every guess costs about a second of Argon2 and the lockout); a
leaked backup file (same); shoulder-surfed or photographed codes (single use);
screenshots/recents thumbnails; cloud backup copies.

What it does **not** protect against:

- A phone with malware or root access while Key Maker is unlocked (it can read
  memory), or a keyboard app that records what you type (use your phone's
  normal keyboard; the app turns off autocorrect/suggestions where it can).
- An attacker who copies the vault file off a rooted phone: the lockout does
  not apply offline, only the password strength does. The TOTP secret is inside
  the vault, so the second factor does **not** help against offline guessing —
  a long password is what matters.
- Someone who edits or deletes `lockout.json` (root needed) or sets the phone
  clock forward can skip the waiting times.
- Someone with your 24 words. They are the signing key.
