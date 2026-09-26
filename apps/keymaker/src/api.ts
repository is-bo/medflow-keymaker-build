import { invoke } from "@tauri-apps/api/core";

/**
 * Typed wrappers over the Rust commands (src-tauri/src/lib.rs). Every security
 * decision is made in Rust; the webview only renders what comes back.
 */

export interface KmError {
  code: string;
  message: string;
  waitSeconds: number | null;
  detail: string | null;
}

export function isKmError(value: unknown): value is KmError {
  return typeof value === "object" && value !== null && "code" in value && "message" in value;
}

const lockListeners = new Set<() => void>();

/** Called whenever Rust reports the session is locked (idle timeout, …). */
export function onLocked(listener: () => void): () => void {
  lockListeners.add(listener);
  return () => lockListeners.delete(listener);
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    const normalized: KmError = isKmError(error)
      ? error
      : { code: "unknown", message: String(error), waitSeconds: null, detail: null };
    if (normalized.code === "locked") lockListeners.forEach((listener) => listener());
    throw normalized;
  }
}

export type Phase = "setup" | "locked" | "unlocked";

export interface Status {
  phase: Phase;
  waitSeconds: number;
  setupFinished: boolean;
  kid: string | null;
  historyCount: number;
  keysSinceBackup: number;
  lastBackupAt: number | null;
  recoveryCodesLeft: number | null;
}

export interface KeyInfo {
  kid: string;
  row: string;
  fingerprint: string;
  publicKeyHex: string;
}

export interface SetupStarted {
  phrase: string[];
  key: KeyInfo;
}

export interface BackupSummary {
  key: KeyInfo;
  historyCount: number;
  createdAt: number;
  lastBackupAt: number | null;
}

export interface TotpEnrollment {
  secret: string;
  uri: string;
  account: string;
}

export interface UnlockResult {
  usedRecoveryCode: boolean;
  recoveryCodesLeft: number;
  setupFinished: boolean;
}

export interface RecoverySheet {
  key: KeyInfo;
  phrase: string[];
  codes: string[];
}

export interface PreparedFile {
  path: string;
  fileName: string;
  contents: string;
}

export interface MachineCheck {
  ok: boolean;
  canonical: string | null;
  error: "empty" | "wrong_length" | "invalid_character" | "checksum_mismatch" | null;
  symbols: number | null;
  position: number | null;
  character: string | null;
}

export interface PhraseCheck {
  wordCount: number;
  unknown: number[];
  valid: boolean;
  error: "word_count" | "unknown_word" | "checksum" | null;
}

export type TermKind = "months" | "years" | "days" | "lifetime";
export interface TermInput {
  kind: TermKind;
  count: number | null;
}

export interface IssueForm {
  licensee: string;
  phone: string;
  machineCode: string;
  term: TermInput;
  telegram: boolean;
  renewalOf: string | null;
}

export type KeyState = "lifetime" | "active" | "expiring" | "expired";

export interface HistoryItem {
  licenseId: string;
  key: string;
  licensee: string;
  phone: string;
  machineCode: string;
  features: string[];
  issuedAt: number;
  expiresAt: number | null;
  termKind: TermKind;
  termCount: number | null;
  kid: string;
  renewalOf: string | null;
  state: KeyState;
  daysLeft: number | null;
}

export type SecondFactor = { kind: "totp"; code: string } | { kind: "recoveryCode"; code: string };

export const api = {
  status: () => call<Status>("status"),
  setupBegin: (password: string, kid: string) => call<SetupStarted>("setup_begin", { password, kid }),
  setupRestorePhrase: (phrase: string, password: string, kid: string) =>
    call<SetupStarted>("setup_restore_phrase", { phrase, password, kid }),
  setupRestoreBackup: (contents: string, password: string) =>
    call<BackupSummary>("setup_restore_backup", { contents, password }),
  setupCheckPhrase: (answers: [number, string][]) => call<void>("setup_check_phrase", { answers }),
  setupNewTotp: () => call<TotpEnrollment>("setup_new_totp"),
  setupConfirmTotp: (code: string) => call<void>("setup_confirm_totp", { code }),
  setupKeepTotp: (code: string) => call<void>("setup_keep_totp", { code }),
  setupCommit: () => call<string[]>("setup_commit"),
  cancelSetup: () => call<void>("cancel_setup"),
  recoverySheet: () => call<RecoverySheet>("recovery_sheet"),
  finishSetup: () => call<void>("finish_setup"),
  unlock: (password: string, factor: SecondFactor) => call<UnlockResult>("unlock", { password, factor }),
  lock: () => call<void>("lock"),
  touch: () => call<void>("touch"),
  eraseVault: () => call<void>("erase_vault"),
  issueKey: (form: IssueForm) => call<HistoryItem>("issue_key", { form }),
  history: (query: string) => call<HistoryItem[]>("history", { query }),
  historyCsvFile: () => call<PreparedFile>("history_csv_file"),
  keyFile: (licenseId: string) => call<PreparedFile>("key_file", { licenseId }),
  checkMachineCode: (input: string) => call<MachineCheck>("check_machine_code", { input }),
  checkPhrase: (input: string) => call<PhraseCheck>("check_phrase", { input }),
  previewExpiry: (term: TermInput) => call<number | null>("preview_expiry", { term }),
  exportBackup: (password: string) => call<PreparedFile>("export_backup", { password }),
  publicKey: () => call<KeyInfo>("public_key"),
  revealPhrase: (password: string) => call<string[]>("reveal_phrase", { password }),
  regenerateRecoveryCodes: (password: string) => call<string[]>("regenerate_recovery_codes", { password }),
  reenrollTotpBegin: (password: string) => call<TotpEnrollment>("reenroll_totp_begin", { password }),
  reenrollTotpConfirm: (code: string) => call<void>("reenroll_totp_confirm", { code }),
  changePassword: (oldPassword: string, newPassword: string) =>
    call<void>("change_password", { oldPassword, newPassword }),
};
