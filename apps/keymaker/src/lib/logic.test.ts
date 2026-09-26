import { describe, expect, it } from "vitest";
import { translate } from "../i18n";
import { en } from "../i18n/en";
import { fr } from "../i18n/fr";
import {
  choiceForTerm,
  formatWait,
  passwordChecklist,
  pickQuizPositions,
  sanitizeTotp,
  termFor,
  termLabel,
} from "./logic";

const t = (path: string, vars?: Record<string, string | number>) => translate("en", path, vars);

describe("password checklist", () => {
  it("mirrors the Rust rules", () => {
    const failing = (pw: string) => passwordChecklist(pw).filter((r) => !r.ok).map((r) => r.rule);
    expect(failing("Tlemcen2026!")).toEqual([]);
    expect(failing("short1")).toEqual(["too_short"]);
    expect(failing("onlyletters")).toEqual(["needs_digit"]);
    expect(failing("12345678901")).toEqual(["needs_letter"]);
    expect(failing("Azerty1234")).toEqual(["too_common"]);
    expect(failing("médecin-2026")).toEqual([]);
  });
});

describe("duration chips", () => {
  it("map to the terms Rust signs", () => {
    expect(termFor("m3", "", "months")).toEqual({ kind: "months", count: 3 });
    expect(termFor("y2", "", "months")).toEqual({ kind: "years", count: 2 });
    expect(termFor("lifetime", "", "months")).toEqual({ kind: "lifetime", count: null });
    expect(termFor("custom", "45", "days")).toEqual({ kind: "days", count: 45 });
    expect(termFor("custom", "0", "days")).toBeNull();
    expect(termFor("custom", "121", "months")).toBeNull();
    expect(termFor("custom", "abc", "days")).toBeNull();
  });

  it("renewals preselect the original length", () => {
    expect(choiceForTerm("months", 6).choice).toBe("m6");
    expect(choiceForTerm("lifetime", null).choice).toBe("lifetime");
    expect(choiceForTerm("days", 45)).toEqual({ choice: "custom", customCount: "45", customUnit: "days" });
    expect(choiceForTerm("years", 3)).toEqual({ choice: "custom", customCount: "36", customUnit: "months" });
  });

  it("labels terms", () => {
    expect(termLabel(t, "months", 1)).toBe("1 month");
    expect(termLabel(t, "years", 2)).toBe("2 years");
    expect(termLabel(t, "lifetime", null)).toBe("Lifetime");
  });
});

describe("misc", () => {
  it("formats lockout waits", () => {
    expect(formatWait(t, 30)).toBe("30 s");
    expect(formatWait(t, 300)).toBe("5 min");
    expect(formatWait(t, 3600)).toBe("1 h");
  });

  it("picks distinct quiz positions", () => {
    for (let i = 0; i < 50; i++) {
      const positions = pickQuizPositions(24, 4);
      expect(new Set(positions).size).toBe(4);
      expect(positions.every((p) => p >= 1 && p <= 24)).toBe(true);
    }
  });

  it("keeps only 6 digits in the code field", () => {
    expect(sanitizeTotp("12 34-56789")).toBe("123456");
  });
});

describe("translations", () => {
  function keys(node: unknown, prefix = ""): string[] {
    if (typeof node === "string") return [prefix];
    return Object.entries(node as Record<string, unknown>).flatMap(([k, v]) => keys(v, prefix ? `${prefix}.${k}` : k));
  }

  it("French has every English key and the same {placeholders}", () => {
    const enKeys = keys(en);
    expect(keys(fr).sort()).toEqual([...enKeys].sort());
    for (const key of enKeys) {
      const vars = (s: string) => (s.match(/\{\w+\}/g) ?? []).sort();
      expect(vars(translate("fr", key)), key).toEqual(vars(translate("en", key)));
    }
  });
});
