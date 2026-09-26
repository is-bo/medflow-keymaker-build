#!/usr/bin/env node
// Hardens the Android project that `tauri android init` generates in
// src-tauri/gen/android. Idempotent: running it twice changes nothing.
//
// Why a patch script instead of committing gen/android: the project is
// generated from the installed Tauri CLI's templates (Gradle/AGP versions,
// build glue), and generating it needs the Android SDK, which only CI has.
// Patching the few security-relevant lines after every `init` keeps the rest in
// step with the CLI while making these guarantees explicit and checked:
//
//   1. RELEASE builds have no INTERNET permission. A release-only manifest
//      (src/release/AndroidManifest.xml) removes it with tools:node="remove",
//      which also strips it if any plugin/library merges it in. Debug builds
//      (`tauri android dev`) keep it: the dev server is reached over the LAN.
//   2. FLAG_SECURE on MainActivity: no screenshots, no screen recording, blank
//      thumbnail in the recent-apps switcher.
//   3. No Android backup / device-to-device transfer of app data
//      (allowBackup=false + data-extraction rules that exclude everything).
//
// Usage: node scripts/patch-android.mjs [path/to/gen/android]
import { existsSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(process.argv[2] ?? join(here, "..", "src-tauri", "gen", "android"));
const appSrc = join(root, "app", "src");
const TOOLS_NS = 'xmlns:tools="http://schemas.android.com/tools"';

function fail(message) {
  console.error(`patch-android: ${message}`);
  process.exit(1);
}

function write(path, contents) {
  mkdirSync(dirname(path), { recursive: true });
  const previous = existsSync(path) ? readFileSync(path, "utf8") : null;
  if (previous !== contents) {
    writeFileSync(path, contents);
    console.log(`patch-android: wrote ${path}`);
  } else {
    console.log(`patch-android: unchanged ${path}`);
  }
}

if (!existsSync(join(appSrc, "main", "AndroidManifest.xml"))) {
  fail(`no generated project at ${root} — run \`tauri android init\` first`);
}

// ── 1 + 3: main manifest (backup off) ─────────────────────────────────────────
const manifestPath = join(appSrc, "main", "AndroidManifest.xml");
let manifest = readFileSync(manifestPath, "utf8");

if (!manifest.includes(TOOLS_NS)) {
  manifest = manifest.replace(/<manifest\b([^>]*?)>/, (_m, attrs) => `<manifest${attrs} ${TOOLS_NS}>`);
}

const appTag = manifest.match(/<application\b[^>]*>/);
if (!appTag) fail("no <application> element in the main manifest");
let application = appTag[0];
function setAttr(tag, name, value) {
  const re = new RegExp(`\\s${name.replace(":", "\\:")}="[^"]*"`);
  return re.test(tag) ? tag.replace(re, ` ${name}="${value}"`) : tag.replace(/<application\b/, `<application ${name}="${value}"`);
}
application = setAttr(application, "android:allowBackup", "false");
application = setAttr(application, "android:dataExtractionRules", "@xml/keymaker_data_extraction_rules");
application = setAttr(application, "tools:replace", "android:allowBackup,android:dataExtractionRules");
manifest = manifest.replace(appTag[0], application);
write(manifestPath, manifest);

write(
  join(appSrc, "main", "res", "xml", "keymaker_data_extraction_rules.xml"),
  `<?xml version="1.0" encoding="utf-8"?>
<!-- Key Maker: nothing leaves the phone through Android backup or device transfer.
     The vault is encrypted anyway; this also keeps the lockout counter local. -->
<data-extraction-rules>
    <cloud-backup>
        <exclude domain="root" path="." />
        <exclude domain="file" path="." />
        <exclude domain="database" path="." />
        <exclude domain="sharedpref" path="." />
        <exclude domain="external" path="." />
    </cloud-backup>
    <device-transfer>
        <exclude domain="root" path="." />
        <exclude domain="file" path="." />
        <exclude domain="database" path="." />
        <exclude domain="sharedpref" path="." />
        <exclude domain="external" path="." />
    </device-transfer>
</data-extraction-rules>
`,
);

// ── 1: release-only manifest removes network permissions ──────────────────────
write(
  join(appSrc, "release", "AndroidManifest.xml"),
  `<?xml version="1.0" encoding="utf-8"?>
<!-- Merged into RELEASE builds only (higher priority than src/main and every
     library manifest). Key Maker never talks to a network. -->
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    ${TOOLS_NS}>
    <uses-permission android:name="android.permission.INTERNET" tools:node="remove" />
    <uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" tools:node="remove" />
    <uses-permission android:name="android.permission.ACCESS_WIFI_STATE" tools:node="remove" />
</manifest>
`,
);

// ── 2: FLAG_SECURE in MainActivity ────────────────────────────────────────────
function findFile(dir, name) {
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      const hit = findFile(full, name);
      if (hit) return hit;
    } else if (entry === name) {
      return full;
    }
  }
  return null;
}
const activityPath = findFile(join(appSrc, "main"), "MainActivity.kt");
if (!activityPath) fail("MainActivity.kt not found");
let activity = readFileSync(activityPath, "utf8");
const SECURE_LINE =
  "window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)";

if (!activity.includes("FLAG_SECURE")) {
  const addImport = (source, fqcn) =>
    source.includes(`import ${fqcn}`) ? source : source.replace(/^(package [^\n]+\n)/, `$1\nimport ${fqcn}\n`);
  activity = addImport(activity, "android.view.WindowManager");
  activity = addImport(activity, "android.os.Bundle");
  const onCreate = /override fun onCreate\(savedInstanceState: Bundle\?\)\s*\{\n/;
  if (onCreate.test(activity)) {
    // Before super.onCreate (and before enableEdgeToEdge): set on the window
    // before any content is drawn.
    activity = activity.replace(onCreate, (m) => `${m}    // Key Maker: block screenshots, screen recording and recents thumbnails.\n    ${SECURE_LINE}\n`);
  } else {
    const cls = /class MainActivity\s*:\s*TauriActivity\(\)\s*(\{\s*\})?/;
    if (!cls.test(activity)) fail("unexpected MainActivity.kt shape; patch FLAG_SECURE by hand");
    activity = activity.replace(
      cls,
      `class MainActivity : TauriActivity() {\n  override fun onCreate(savedInstanceState: Bundle?) {\n    // Key Maker: block screenshots, screen recording and recents thumbnails.\n    ${SECURE_LINE}\n    super.onCreate(savedInstanceState)\n  }\n}`,
    );
  }
}
write(activityPath, activity);

// ── Self-check: every guarantee is present in the files ───────────────────────
const checks = [
  [manifestPath, 'android:allowBackup="false"'],
  [manifestPath, "@xml/keymaker_data_extraction_rules"],
  [join(appSrc, "release", "AndroidManifest.xml"), 'android.permission.INTERNET" tools:node="remove"'],
  [activityPath, SECURE_LINE],
];
for (const [path, needle] of checks) {
  if (!readFileSync(path, "utf8").includes(needle)) fail(`${path} is missing ${needle}`);
}
const secureAt = activity.indexOf(SECURE_LINE);
const superAt = activity.indexOf("super.onCreate");
if (superAt >= 0 && secureAt > superAt) fail("FLAG_SECURE must be set before super.onCreate");
console.log("patch-android: OK (release without INTERNET, FLAG_SECURE, backup disabled)");
