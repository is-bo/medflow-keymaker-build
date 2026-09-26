import { useEffect, useState } from "react";
import { api, isKmError, type UnlockResult } from "../api";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { formatWait } from "../lib/logic";
import { LockIcon } from "../ui/icons";
import { Banner, Button, CodeField, ErrorLine, Field, LanguageSwitch, PasswordField } from "../ui/kit";

/**
 * Password + 6-digit code (or a single-use recovery code). The countdown comes
 * from Rust's persisted lockout, so restarting the app does not reset it.
 */
export function Unlock({ initialWait, onUnlocked, onErased }: { initialWait: number; onUnlocked: (r: UnlockResult) => void; onErased: () => void }) {
  const { t } = useI18n();
  const [password, setPassword] = useState("");
  const [code, setCode] = useState("");
  const [recovery, setRecovery] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [waitUntil, setWaitUntil] = useState(() => Date.now() + initialWait * 1000);
  const [now, setNow] = useState(Date.now());
  const [erasing, setErasing] = useState(false);
  const [eraseText, setEraseText] = useState("");

  const waitSeconds = Math.max(0, Math.ceil((waitUntil - now) / 1000));
  useEffect(() => {
    if (waitSeconds <= 0) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [waitSeconds]);

  const codeReady = recovery ? code.replace(/[^0-9A-Za-z]/g, "").length === 10 : code.length === 6;

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      const result = await api.unlock(password, recovery ? { kind: "recoveryCode", code } : { kind: "totp", code });
      setPassword("");
      onUnlocked(result);
    } catch (e) {
      setError(errorText(t, e));
      setCode("");
      if (isKmError(e) && e.waitSeconds) {
        setWaitUntil(Date.now() + e.waitSeconds * 1000);
        setNow(Date.now());
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col bg-surface-app">
      <header className="shrink-0 bg-chrome pt-[env(safe-area-inset-top)]">
        <div className="mx-auto flex w-full max-w-md items-center justify-between px-4 pb-6 pt-4">
          <span className="flex items-center gap-2 text-[15px] font-extrabold text-white">
            <span className="flex h-9 w-9 items-center justify-center rounded-xl bg-chrome-soft text-mint-bright">
              <LockIcon size={20} />
            </span>
            {t("app.name")}
          </span>
          <LanguageSwitch />
        </div>
      </header>
      <main className="min-h-0 flex-1 overflow-y-auto">
        <form
          className="mx-auto w-full max-w-md space-y-4 px-4 pb-10 pt-6"
          onSubmit={(e) => {
            e.preventDefault();
            if (!busy && waitSeconds === 0 && password && codeReady) void submit();
          }}
        >
          <div>
            <h1 className="text-[24px] font-extrabold tracking-tight text-ink">{t("unlock.title")}</h1>
            <p className="mt-1 text-[15px] text-ink-body">{t("unlock.body")}</p>
          </div>
          {waitSeconds > 0 && <Banner tone="warning">{t("unlock.waiting", { wait: formatWait(t, waitSeconds) })}</Banner>}
          <PasswordField label={t("password.label")} value={password} onChange={(e) => setPassword(e.target.value)} autoFocus />
          {recovery ? (
            <Field
              label={t("unlock.recoveryCode")}
              value={code}
              onChange={(e) => setCode(e.target.value.toUpperCase().slice(0, 12))}
              placeholder="XXXXX-XXXXX"
              autoCapitalize="characters"
              autoCorrect="off"
              autoComplete="off"
              spellCheck={false}
              className="[&_input]:font-mono [&_input]:tracking-widest"
            />
          ) : (
            <CodeField label={t("unlock.code")} value={code} onChange={setCode} />
          )}
          <ErrorLine message={error} />
          <Button type="submit" variant="primary" loading={busy} disabled={waitSeconds > 0 || !password || !codeReady}>
            {busy ? t("unlock.checking") : t("unlock.submit")}
          </Button>
          <Button
            variant="ghost"
            onClick={() => {
              setRecovery((r) => !r);
              setCode("");
              setError(null);
            }}
          >
            {recovery ? t("unlock.useTotp") : t("unlock.useRecovery")}
          </Button>

          <div className="border-t border-line-strong pt-4">
            {!erasing ? (
              <button type="button" onClick={() => setErasing(true)} className="min-h-11 px-1 text-[14px] font-bold text-ink-soft">
                {t("unlock.forgot")}
              </button>
            ) : (
              <div className="space-y-3 rounded-card border border-danger-border bg-danger-surface p-4">
                <p className="text-[15px] font-extrabold text-danger">{t("unlock.eraseTitle")}</p>
                <p className="text-[14px] leading-relaxed text-ink-body">{t("unlock.eraseBody")}</p>
                <Field label={t("unlock.eraseType")} value={eraseText} onChange={(e) => setEraseText(e.target.value)} autoCapitalize="characters" autoComplete="off" />
                <Button
                  variant="danger"
                  disabled={eraseText.trim().toUpperCase() !== t("unlock.eraseWord")}
                  onClick={async () => {
                    try {
                      await api.eraseVault();
                      onErased();
                    } catch (e) {
                      setError(errorText(t, e));
                    }
                  }}
                >
                  {t("unlock.eraseButton")}
                </Button>
                <Button variant="ghost" onClick={() => setErasing(false)}>
                  {t("app.cancel")}
                </Button>
              </div>
            )}
          </div>
        </form>
      </main>
    </div>
  );
}
