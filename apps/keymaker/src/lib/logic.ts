import type { TermInput, TermKind } from "../api";
import type { TranslateFn } from "../i18n";

/**
 * Pure UI logic, unit-tested in logic.test.ts. Rust re-checks everything
 * (password rules, terms, machine codes); these only drive live feedback.
 */

export type PasswordRule = "too_short" | "needs_letter" | "needs_digit" | "too_common";

const COMMON = new Set([
  "password12", "password123", "motdepasse1", "azerty1234", "azertyuiop1", "qwerty1234",
  "1234567890", "0123456789", "medflow123", "medflow2026", "keymaker123", "algerie2026",
]);

/** Mirrors core/src/rules.rs `password_problems` (minus the 256-char cap). */
export function passwordChecklist(password: string): { rule: PasswordRule; ok: boolean }[] {
  const lower = password.toLowerCase();
  const chars = [...password];
  const allSame = chars.length > 0 && [...lower].every((c) => c === [...lower][0]);
  return [
    { rule: "too_short", ok: chars.length >= 10 },
    { rule: "needs_letter", ok: /\p{L}/u.test(password) },
    { rule: "needs_digit", ok: /[0-9]/.test(password) },
    { rule: "too_common", ok: password.length > 0 && !allSame && !COMMON.has(lower) },
  ];
}

export type DurationChoice = "m1" | "m3" | "m6" | "y1" | "y2" | "lifetime" | "custom";
export const DURATION_CHOICES: DurationChoice[] = ["m1", "m3", "m6", "y1", "y2", "lifetime", "custom"];

export type CustomUnit = "months" | "days";

/** Chip (+ custom fields) → the term Rust signs; null while custom is incomplete. */
export function termFor(choice: DurationChoice, customCount: string, customUnit: CustomUnit): TermInput | null {
  switch (choice) {
    case "m1":
      return { kind: "months", count: 1 };
    case "m3":
      return { kind: "months", count: 3 };
    case "m6":
      return { kind: "months", count: 6 };
    case "y1":
      return { kind: "years", count: 1 };
    case "y2":
      return { kind: "years", count: 2 };
    case "lifetime":
      return { kind: "lifetime", count: null };
    case "custom": {
      if (!/^\d{1,4}$/.test(customCount.trim())) return null;
      const count = Number(customCount.trim());
      const max = customUnit === "months" ? 120 : 3650;
      return count >= 1 && count <= max ? { kind: customUnit, count } : null;
    }
  }
}

/** Best chip for a term being renewed (so "Renew" preselects the same length). */
export function choiceForTerm(kind: TermKind, count: number | null): { choice: DurationChoice; customCount: string; customUnit: CustomUnit } {
  const preset: Record<string, DurationChoice> = { "months:1": "m1", "months:3": "m3", "months:6": "m6", "years:1": "y1", "years:2": "y2" };
  if (kind === "lifetime") return { choice: "lifetime", customCount: "", customUnit: "months" };
  const hit = preset[`${kind}:${count}`];
  if (hit) return { choice: hit, customCount: "", customUnit: "months" };
  if (kind === "years" && count) return { choice: "custom", customCount: String(count * 12), customUnit: "months" };
  return { choice: "custom", customCount: count ? String(count) : "", customUnit: kind === "days" ? "days" : "months" };
}

export function termLabel(t: TranslateFn, kind: TermKind, count: number | null): string {
  if (kind === "lifetime" || count === null) return t("term.lifetime");
  const singular = { months: "term.month", years: "term.year", days: "term.day" }[kind];
  return count === 1 ? t(singular) : t(`term.${kind}`, { n: count });
}

/** 30 → "30 s", 300 → "5 min", 3600 → "1 h" (rounded up). */
export function formatWait(t: TranslateFn, seconds: number): string {
  if (seconds < 60) return t("time.seconds", { n: Math.max(1, Math.ceil(seconds)) });
  if (seconds < 3600) return t("time.minutes", { n: Math.ceil(seconds / 60) });
  return t("time.hours", { n: Math.ceil(seconds / 3600) });
}

/** `count` distinct 1-based positions out of `total`, sorted. */
export function pickQuizPositions(total: number, count: number, random: () => number = Math.random): number[] {
  const picked = new Set<number>();
  while (picked.size < Math.min(count, total)) picked.add(1 + Math.floor(random() * total));
  return [...picked].sort((a, b) => a - b);
}

/** Only digits, at most 6 — what the TOTP field accepts while typing. */
export function sanitizeTotp(input: string): string {
  return input.replace(/\D/g, "").slice(0, 6);
}
