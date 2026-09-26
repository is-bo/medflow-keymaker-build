import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * Hand-offs to other apps (share sheet, file picker, authenticator). Each one
 * puts Key Maker in the background; `external()` marks that as expected so the
 * lock-on-background rule does not fire mid-share. The Rust 2-minute idle
 * timeout still applies, so a long detour still ends locked.
 */
let externalDepth = 0;

export function isExternalActive(): boolean {
  return externalDepth > 0;
}

export async function external<T>(action: () => Promise<T>): Promise<T> {
  externalDepth += 1;
  try {
    return await action();
  } finally {
    // Let the visibilitychange of the return trip land before re-arming.
    window.setTimeout(() => {
      externalDepth = Math.max(0, externalDepth - 1);
    }, 800);
  }
}

/**
 * Keep the session open while this screen is mounted even if the app goes to
 * the background — only for authenticator enrollment, where the owner must
 * switch to the authenticator app to read the code. Returns the release.
 */
export function holdForExternalApp(): () => void {
  externalDepth += 1;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    externalDepth = Math.max(0, externalDepth - 1);
  };
}

/** The share plugin rejects with "Share cancelled" even after a successful
 *  share on many Android versions (the chooser returns RESULT_CANCELED). */
function isCancel(error: unknown): boolean {
  return /cancel/i.test(String((error as { message?: string })?.message ?? error));
}

export type ShareOutcome = "shared" | "unavailable";

export async function shareText(text: string): Promise<ShareOutcome> {
  try {
    await external(() => invoke("plugin:sharekit|share_text", { text, mimeType: "text/plain" }));
    return "shared";
  } catch (error) {
    return isCancel(error) ? "shared" : "unavailable";
  }
}

export async function shareFile(path: string, mimeType: string, title: string): Promise<ShareOutcome> {
  try {
    await external(() => invoke("plugin:sharekit|share_file", { url: path, mimeType, title }));
    return "shared";
  } catch (error) {
    return isCancel(error) ? "shared" : "unavailable";
  }
}

export async function copyText(text: string): Promise<void> {
  await writeText(text);
}

/** System "save as" picker; false when the user backed out. */
export async function saveTextFile(fileName: string, contents: string, extension: string): Promise<boolean> {
  const target = await external(() =>
    saveDialog({ defaultPath: fileName, filters: [{ name: extension.toUpperCase(), extensions: [extension] }] }),
  );
  if (!target) return false;
  await writeTextFile(target, contents);
  return true;
}

/** System "open" picker; null when the user backed out. */
export async function pickTextFile(): Promise<{ name: string; contents: string } | null> {
  const picked = await external(() => openDialog({ multiple: false, directory: false }));
  if (!picked || Array.isArray(picked)) return null;
  const contents = await readTextFile(picked);
  const name = decodeURIComponent(picked.split(/[\\/]/).pop() ?? picked);
  return { name, contents };
}

export async function openAuthenticator(uri: string): Promise<boolean> {
  try {
    await external(() => openUrl(uri));
    return true;
  } catch {
    return false;
  }
}
