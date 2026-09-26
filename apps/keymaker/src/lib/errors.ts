import { isKmError } from "../api";
import type { TranslateFn } from "../i18n";
import { formatWait } from "./logic";

/** A Rust error (or anything thrown) as one translated sentence. */
export function errorText(t: TranslateFn, error: unknown): string {
  if (!isKmError(error)) return t("errors.generic", { message: String(error) });
  const wait = error.waitSeconds ?? 0;
  switch (error.code) {
    case "wrong_credentials":
      return wait > 0 ? t("errors.wrong_credentials_wait", { wait: formatWait(t, wait) }) : t("errors.wrong_credentials");
    case "too_many_attempts":
      return t("errors.too_many_attempts", { wait: formatWait(t, wait) });
    case "storage_failed":
    case "issue_failed":
      return t(`errors.${error.code}`, { message: error.detail ?? error.message });
    case "bad_totp_code":
    case "phrase_invalid":
    case "phrase_mismatch":
    case "backup_invalid":
    case "backup_wrong_password":
    case "weak_password":
    case "bad_kid":
    case "machine_code_invalid":
    case "invalid_input":
    case "vault_corrupt":
    case "vault_exists":
    case "locked":
    case "setup_order":
      return t(`errors.${error.code}`);
    default:
      return t("errors.generic", { message: error.message });
  }
}
