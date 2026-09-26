import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState } from "react";
import { api, type KeyInfo, type TotpEnrollment } from "../api";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { AlertIcon, ChevronIcon, KeyIcon, LockIcon, ShieldIcon } from "../ui/icons";
import { Banner, Button, Card, ErrorLine, LanguageSwitch, Lead, PasswordField, Row, Screen, SectionTitle, WordGrid } from "../ui/kit";
import { NewPasswordFields, PublicKeyCard, TotpEnroll, passwordReady } from "./shared";
import { CodeList } from "./SetupFlow";

type Panel = null | "publicKey" | "phrase" | "codes" | "totp" | "password";

/** Settings: public key, and the password-gated security actions. */
export function Settings({ onBack, onLock }: { onBack: () => void; onLock: () => void }) {
  const { t } = useI18n();
  const [panel, setPanel] = useState<Panel>(null);
  const [info, setInfo] = useState<KeyInfo | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const [version, setVersion] = useState("");

  useEffect(() => {
    api.publicKey().then(setInfo, () => undefined);
    getVersion().then(setVersion, () => undefined);
  }, []);

  if (panel === "publicKey" && info) {
    return (
      <Screen title={t("settings.publicKey")} onBack={() => setPanel(null)} onLock={onLock}>
        <PublicKeyCard info={info} />
      </Screen>
    );
  }
  if (panel === "phrase") return <RevealPhrase onBack={() => setPanel(null)} onLock={onLock} />;
  if (panel === "codes") return <NewCodes onBack={() => setPanel(null)} onLock={onLock} />;
  if (panel === "totp")
    return (
      <NewAuthenticator
        onBack={() => setPanel(null)}
        onLock={onLock}
        onDone={() => {
          setDone(t("settings.totpChanged"));
          setPanel(null);
        }}
      />
    );
  if (panel === "password")
    return (
      <ChangePassword
        onBack={() => setPanel(null)}
        onLock={onLock}
        onDone={() => {
          setDone(t("settings.passwordChanged"));
          setPanel(null);
        }}
      />
    );

  const chevron = <ChevronIcon className="text-ink-faint" />;
  return (
    <Screen title={t("settings.title")} onBack={onBack} onLock={onLock}>
      {done && <Banner tone="success">{done}</Banner>}
      <div className="flex items-center justify-between rounded-card border border-line bg-chrome px-4 py-3">
        <span className="text-[15px] font-bold text-white">{t("app.language")}</span>
        <LanguageSwitch />
      </div>
      <Row
        icon={<KeyIcon />}
        title={t("settings.publicKey")}
        subtitle={info ? `${info.kid} · ${info.fingerprint}` : undefined}
        onClick={() => setPanel("publicKey")}
        trailing={chevron}
      />
      <SectionTitle>{t("settings.security")}</SectionTitle>
      <Row icon={<AlertIcon />} title={t("settings.showPhrase")} subtitle={t("settings.showPhraseBody")} onClick={() => setPanel("phrase")} trailing={chevron} />
      <Row icon={<ShieldIcon />} title={t("settings.newCodes")} subtitle={t("settings.newCodesBody")} onClick={() => setPanel("codes")} trailing={chevron} />
      <Row
        icon={<ShieldIcon />}
        title={t("settings.newAuthenticator")}
        subtitle={t("settings.newAuthenticatorBody")}
        onClick={() => setPanel("totp")}
        trailing={chevron}
      />
      <Row icon={<LockIcon />} title={t("settings.changePassword")} onClick={() => setPanel("password")} trailing={chevron} />
      <Card className="space-y-1 border-warning-border bg-warning-surface">
        <p className="text-[14px] font-extrabold text-warning">{t("settings.compromiseTitle")}</p>
        <p className="text-[13.5px] leading-relaxed text-ink-body">{t("settings.compromiseBody")}</p>
      </Card>
      <p className="px-1 pt-2 text-center text-[12px] text-ink-soft">{t("settings.about", { version, major: 2 })}</p>
    </Screen>
  );
}

/** Password prompt shared by the gated actions below. */
function PasswordGate({ busy, error, onSubmit, label }: { busy: boolean; error: string | null; onSubmit: (pw: string) => void; label?: string }) {
  const { t } = useI18n();
  const [password, setPassword] = useState("");
  return (
    <form
      className="space-y-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (password) onSubmit(password);
      }}
    >
      <PasswordField label={label ?? t("settings.confirmPassword")} value={password} onChange={(e) => setPassword(e.target.value)} />
      <ErrorLine message={error} />
      <Button type="submit" variant="primary" loading={busy} disabled={!password}>
        {t("app.next")}
      </Button>
    </form>
  );
}

function useAction() {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function run<T>(action: () => Promise<T>): Promise<T | undefined> {
    setBusy(true);
    setError(null);
    try {
      return await action();
    } catch (e) {
      setError(errorText(t, e));
      return undefined;
    } finally {
      setBusy(false);
    }
  }
  return { busy, error, run };
}

function RevealPhrase({ onBack, onLock }: { onBack: () => void; onLock: () => void }) {
  const { t } = useI18n();
  const { busy, error, run } = useAction();
  const [words, setWords] = useState<string[] | null>(null);
  return (
    <Screen title={t("settings.showPhrase")} onBack={onBack} onLock={onLock}>
      <Banner tone="danger">{t("setup.phrase.warning")}</Banner>
      {words ? (
        <WordGrid words={words} />
      ) : (
        <PasswordGate busy={busy} error={error} onSubmit={async (pw) => setWords((await run(() => api.revealPhrase(pw))) ?? null)} />
      )}
    </Screen>
  );
}

function NewCodes({ onBack, onLock }: { onBack: () => void; onLock: () => void }) {
  const { t } = useI18n();
  const { busy, error, run } = useAction();
  const [codes, setCodes] = useState<string[] | null>(null);
  return (
    <Screen title={t("settings.newCodes")} onBack={onBack} onLock={onLock}>
      <Lead>{codes ? t("setup.codes.body") : t("settings.newCodesBody")}</Lead>
      {codes ? (
        <>
          <CodeList codes={codes} />
          <Button variant="primary" onClick={onBack}>
            {t("setup.codes.written")}
          </Button>
        </>
      ) : (
        <PasswordGate busy={busy} error={error} onSubmit={async (pw) => setCodes((await run(() => api.regenerateRecoveryCodes(pw))) ?? null)} />
      )}
    </Screen>
  );
}

function NewAuthenticator({ onBack, onLock, onDone }: { onBack: () => void; onLock: () => void; onDone: () => void }) {
  const { t } = useI18n();
  const { busy, error, run } = useAction();
  const [enrollment, setEnrollment] = useState<TotpEnrollment | null>(null);
  const [code, setCode] = useState("");
  return (
    <Screen
      title={t("settings.newAuthenticator")}
      onBack={onBack}
      onLock={onLock}
      footer={
        enrollment ? (
          <Button
            variant="primary"
            loading={busy}
            disabled={code.length !== 6}
            onClick={async () => {
              const ok = await run(async () => {
                await api.reenrollTotpConfirm(code);
                return true;
              });
              if (ok) onDone();
            }}
          >
            {t("setup.totp.confirm")}
          </Button>
        ) : undefined
      }
    >
      <Lead>{t("setup.totp.body")}</Lead>
      {enrollment ? (
        <TotpEnroll enrollment={enrollment} code={code} onCode={setCode} error={error} />
      ) : (
        <PasswordGate busy={busy} error={error} onSubmit={async (pw) => setEnrollment((await run(() => api.reenrollTotpBegin(pw))) ?? null)} />
      )}
    </Screen>
  );
}

function ChangePassword({ onBack, onLock, onDone }: { onBack: () => void; onLock: () => void; onDone: () => void }) {
  const { t } = useI18n();
  const { busy, error, run } = useAction();
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [confirm, setConfirm] = useState("");
  return (
    <Screen
      title={t("settings.changePassword")}
      onBack={onBack}
      onLock={onLock}
      footer={
        <Button
          variant="primary"
          loading={busy}
          disabled={!current || !passwordReady(next, confirm)}
          onClick={async () => {
            const ok = await run(async () => {
              await api.changePassword(current, next);
              return true;
            });
            if (ok) onDone();
          }}
        >
          {t("settings.changePassword")}
        </Button>
      }
    >
      <PasswordField label={t("settings.currentPassword")} value={current} onChange={(e) => setCurrent(e.target.value)} />
      <NewPasswordFields label={t("settings.newPassword")} password={next} confirm={confirm} onPassword={setNext} onConfirm={setConfirm} />
      <ErrorLine message={error} />
    </Screen>
  );
}
