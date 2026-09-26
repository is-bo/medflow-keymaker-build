import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { en, type Dict } from "./en";
import { fr } from "./fr";

/**
 * Same conventions as the desktop's i18n.tsx: dotted-path lookup, `{var}`
 * interpolation, English fallback. Key Maker ships English + French only
 * (Arabic was optional for this owner-only tool).
 */
export type Locale = "en" | "fr";
const dictionaries: Record<Locale, Dict> = { en, fr };
const STORAGE_KEY = "MedFlowKeyMaker-lang";

export const localeToIntl: Record<Locale, string> = { en: "en-GB", fr: "fr-DZ" };

export type TranslateFn = (path: string, vars?: Record<string, string | number>) => string;

function resolvePath(source: unknown, path: string): string | undefined {
  let node = source;
  for (const segment of path.split(".")) {
    if (typeof node !== "object" || node === null) return undefined;
    node = (node as Record<string, unknown>)[segment];
  }
  return typeof node === "string" ? node : undefined;
}

export function translate(locale: Locale, path: string, vars?: Record<string, string | number>): string {
  let text = resolvePath(dictionaries[locale], path) ?? resolvePath(en, path) ?? path;
  if (vars) {
    for (const [key, value] of Object.entries(vars)) text = text.replaceAll(`{${key}}`, String(value));
  }
  return text;
}

interface I18nValue {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: TranslateFn;
  formatDate: (unixMs: number) => string;
}

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(() =>
    localStorage.getItem(STORAGE_KEY) === "en" ? "en" : "fr",
  );
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);
  const setLocale = useCallback((next: Locale) => {
    localStorage.setItem(STORAGE_KEY, next);
    setLocaleState(next);
  }, []);
  const t = useCallback<TranslateFn>((path, vars) => translate(locale, path, vars), [locale]);
  const formatDate = useCallback(
    (unixMs: number) =>
      new Intl.DateTimeFormat(localeToIntl[locale], { day: "numeric", month: "long", year: "numeric", timeZone: "UTC" }).format(
        new Date(unixMs),
      ),
    [locale],
  );
  const value = useMemo(() => ({ locale, setLocale, t, formatDate }), [locale, setLocale, t, formatDate]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error("useI18n outside I18nProvider");
  return value;
}
