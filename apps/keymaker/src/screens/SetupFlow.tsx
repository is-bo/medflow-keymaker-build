import { useEffect, useMemo, useState } from "react";
import { api, type BackupSummary, type PhraseCheck, type PreparedFile, type RecoverySheet, type SetupStarted, type TotpEnrollment } from "../api";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { pickQuizPositions } from "../lib/logic";
import { pickTextFile, saveTextFile, shareFile } from "../platform";
import { CloudIcon, DownloadIcon, HistoryIcon, KeyIcon, ShareIcon } from "../ui/icons";
import { Banner, Button, Card, CodeField, Confirm, ErrorLine, Field, Lead, PasswordField, Row, Screen, Steps, WordGrid } from "../ui/kit";
import { NewPasswordFields, PublicKeyCard, TotpEnroll, passwordReady } from "./shared";

/**
 * The whole first-run story, in three variants:
 * - `new`: password → 24 words → word check → authenticator → (vault written)
 *   → recovery codes → recovery sheet → forced backup → public key
 * - `restore`: backup file or 24 words → authenticator (keep or new) → the
 *   same post-commit steps
 * - `resume`: the app restarted after the vault was written but before the
 *   last step — continues at the recovery codes (freshly re-issued).
 */
type Step =
  | "password"
  | "phrase"
  | "quiz"
  | "restoreChoose"
  | "restoreBackup"
  | "restorePhrase"
  | "totpChoice"
  | "totp"
  | "codes"
  | "sheet"
  | "backup"
  | "publicKey";

const NEW_STEPS: Step[] = ["password", "phrase", "quiz", "totp", "codes", "sheet", "backup", "publicKey"];
const RESTORE_STEPS: Step[] = ["restoreChoose", "restoreBackup", "totp", "codes", "sheet", "backup", "publicKey"];

function defaultKid(): string {
  const now = new Date();
  return `prod-${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
}

export function SetupFlow({ mode, onExit, onFinished }: { mode: "new" | "restore" | "resume"; onExit: () => void; onFinished: () => void }) {
  const { t } = useI18n();
  const [step, setStep] = useState<Step>(mode === "new" ? "password" : mode === "restore" ? "restoreChoose" : "codes");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Pre-commit state
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [kid, setKid] = useState(defaultKid);
  const [showKid, setShowKid] = useState(false);
  const [started, setStarted] = useState<SetupStarted | null>(null);
  const [phraseWritten, setPhraseWritten] = useState(false);
  const [quizPositions, setQuizPositions] = useState<number[]>([]);
  const [quizAnswers, setQuizAnswers] = useState<Record<number, string>>({});
  const [enrollment, setEnrollment] = useState<TotpEnrollment | null>(null);
  const [code, setCode] = useState("");
  // Restore state
  const [backupFile, setBackupFile] = useState<{ name: string; contents: string } | null>(null);
  const [summary, setSummary] = useState<BackupSummary | null>(null);
  const [phraseInput, setPhraseInput] = useState("");
  const [phraseCheck, setPhraseCheck] = useState<PhraseCheck | null>(null);
  // Post-commit state
  const [sheet, setSheet] = useState<RecoverySheet | null>(null);
  const [codesKept, setCodesKept] = useState(false);
  const [codesWritten, setCodesWritten] = useState(false);
  const [sheetWritten, setSheetWritten] = useState(false);
  const [backupPassword, setBackupPassword] = useState("");
  const [backup, setBackup] = useState<PreparedFile | null>(null);
  const [backupSaved, setBackupSaved] = useState(false);
  const [backupNote, setBackupNote] = useState<string | null>(null);

  const order = mode === "new" ? NEW_STEPS : RESTORE_STEPS;
  const position = useMemo(() => {
    const normalized: Step = step === "restorePhrase" ? "restoreBackup" : step === "totpChoice" ? "totp" : step;
    const i = order.indexOf(normalized);
    return i < 0 ? order.length : i + 1;
  }, [order, step]);

  function go(next: Step) {
    setError(null);
    setStep(next);
  }

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

  // Resume: fetch the sheet (with freshly issued codes) straight away.
  useEffect(() => {
    if (mode !== "resume") return;
    void run(async () => setSheet(await api.recoverySheet()));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode]);

  // Live check of the typed phrase (Rust validates words + checksum).
  useEffect(() => {
    if (step !== "restorePhrase" || !phraseInput.trim()) {
      setPhraseCheck(null);
      return;
    }
    const timer = window.setTimeout(() => {
      api.checkPhrase(phraseInput).then(setPhraseCheck, () => setPhraseCheck(null));
    }, 250);
    return () => window.clearTimeout(timer);
  }, [phraseInput, step]);

  async function startTotp() {
    const e = await run(() => api.setupNewTotp());
    if (e) {
      setEnrollment(e);
      setCode("");
      go("totp");
    }
  }

  async function commit() {
    const codes = await run(() => api.setupCommit());
    if (!codes) return;
    setPassword("");
    setConfirm("");
    const s = await run(() => api.recoverySheet());
    if (!s) return;
    setSheet(s);
    setCodesKept(codes.length === 0);
    go(codes.length === 0 ? "sheet" : "codes");
  }

  function exit() {
    void api.cancelSetup().catch(() => undefined);
    onExit();
  }

  const back: Partial<Record<Step, () => void>> = {
    password: exit,
    phrase: () => go("password"),
    quiz: () => go("phrase"),
    restoreChoose: exit,
    restoreBackup: () => go("restoreChoose"),
    restorePhrase: () => go("restoreChoose"),
    totpChoice: () => go("restoreBackup"),
    totp: () => go(mode === "new" ? "quiz" : summary ? "totpChoice" : "restorePhrase"),
    sheet: codesKept ? undefined : () => go("codes"),
    backup: () => go("sheet"),
    publicKey: () => go("backup"),
  };

  const progress = <Steps current={position} total={order.length} />;

  // ── Pre-commit: new setup ──────────────────────────────────────────────────

  if (step === "password") {
    const ready = passwordReady(password, confirm) && /^[a-z0-9][a-z0-9-]{2,31}$/.test(kid);
    return (
      <Screen
        title={t("setup.password.title")}
        onBack={back.password}
        footer={
          <Button
            variant="primary"
            loading={busy}
            disabled={!ready}
            onClick={async () => {
              const s = await run(() => api.setupBegin(password, kid));
              if (s) {
                setStarted(s);
                setPhraseWritten(false);
                go("phrase");
              }
            }}
          >
            {busy ? t("setup.password.creating") : t("setup.password.create")}
          </Button>
        }
      >
        {progress}
        <Lead>{t("setup.password.body")}</Lead>
        <NewPasswordFields password={password} confirm={confirm} onPassword={setPassword} onConfirm={setConfirm} />
        {showKid ? (
          <Field
            label={t("setup.password.kidLabel")}
            hint={t("setup.password.kidHint")}
            value={kid}
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            onChange={(e) => setKid(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ""))}
          />
        ) : (
          <button type="button" onClick={() => setShowKid(true)} className="min-h-11 px-1 text-[14px] font-bold text-primary">
            {t("setup.password.advanced")} · {kid}
          </button>
        )}
        <ErrorLine message={error} />
      </Screen>
    );
  }

  if (step === "phrase" && started) {
    return (
      <Screen
        title={t("setup.phrase.title")}
        onBack={back.phrase}
        footer={
          <Button
            variant="primary"
            disabled={!phraseWritten}
            onClick={() => {
              setQuizPositions(pickQuizPositions(24, 4));
              setQuizAnswers({});
              go("quiz");
            }}
          >
            {t("app.next")}
          </Button>
        }
      >
        {progress}
        <Banner tone="danger">{t("setup.phrase.warning")}</Banner>
        <Lead>{t("setup.phrase.body")}</Lead>
        <WordGrid words={started.phrase} />
        <Confirm checked={phraseWritten} onChange={setPhraseWritten}>
          {t("setup.phrase.written")}
        </Confirm>
      </Screen>
    );
  }

  if (step === "quiz") {
    const complete = quizPositions.every((p) => (quizAnswers[p] ?? "").trim().length > 0);
    return (
      <Screen
        title={t("setup.quiz.title")}
        onBack={back.quiz}
        footer={
          <Button
            variant="primary"
            loading={busy}
            disabled={!complete}
            onClick={async () => {
              const answers: [number, string][] = quizPositions.map((p) => [p, quizAnswers[p] ?? ""]);
              const ok = await run(async () => {
                await api.setupCheckPhrase(answers);
                return true;
              });
              if (ok) await startTotp();
            }}
          >
            {t("setup.quiz.check")}
          </Button>
        }
      >
        {progress}
        <Lead>{t("setup.quiz.body")}</Lead>
        {quizPositions.map((p) => (
          <Field
            key={p}
            label={t("setup.quiz.word", { n: p })}
            value={quizAnswers[p] ?? ""}
            autoCapitalize="off"
            autoCorrect="off"
            autoComplete="off"
            spellCheck={false}
            className="[&_input]:font-mono"
            onChange={(e) => setQuizAnswers((a) => ({ ...a, [p]: e.target.value }))}
          />
        ))}
        <ErrorLine message={error} />
        {error && (
          <Button variant="ghost" onClick={() => go("phrase")}>
            {t("setup.quiz.showAgain")}
          </Button>
        )}
      </Screen>
    );
  }

  // ── Pre-commit: restore ────────────────────────────────────────────────────

  if (step === "restoreChoose") {
    return (
      <Screen title={t("restore.title")} onBack={back.restoreChoose}>
        <Lead>{t("restore.choose")}</Lead>
        <Row icon={<CloudIcon />} title={t("restore.fromBackup")} subtitle={t("restore.fromBackupHint")} onClick={() => go("restoreBackup")} />
        <Row icon={<KeyIcon />} title={t("restore.fromPhrase")} subtitle={t("restore.fromPhraseHint")} onClick={() => go("restorePhrase")} />
      </Screen>
    );
  }

  if (step === "restoreBackup") {
    return (
      <Screen
        title={t("restore.fromBackup")}
        onBack={back.restoreBackup}
        footer={
          <Button
            variant="primary"
            loading={busy}
            disabled={!backupFile || password.length === 0}
            onClick={async () => {
              const s = await run(() => api.setupRestoreBackup(backupFile!.contents, password));
              if (s) {
                setSummary(s);
                setCode("");
                go("totpChoice");
              }
            }}
          >
            {t("restore.open")}
          </Button>
        }
      >
        {progress}
        <Button
          variant="secondary"
          icon={<HistoryIcon size={20} />}
          onClick={async () => {
            const file = await run(() => pickTextFile());
            if (file) setBackupFile(file);
          }}
        >
          {t("restore.pickFile")}
        </Button>
        {backupFile && <p className="break-all px-1 text-[14px] font-semibold text-ink-body">{t("restore.fileChosen", { name: backupFile.name })}</p>}
        <PasswordField label={t("restore.backupPassword")} value={password} onChange={(e) => setPassword(e.target.value)} />
        <ErrorLine message={error} />
      </Screen>
    );
  }

  if (step === "restorePhrase") {
    const ready = phraseCheck?.valid === true && passwordReady(password, confirm) && /^[a-z0-9][a-z0-9-]{2,31}$/.test(kid);
    let phraseStatus: { tone: "ok" | "bad" | "neutral"; text: string } | null = null;
    if (phraseCheck) {
      if (phraseCheck.valid) phraseStatus = { tone: "ok", text: t("restore.phraseOk") };
      else if (phraseCheck.unknown.length > 0) phraseStatus = { tone: "bad", text: t("restore.phraseUnknown", { list: phraseCheck.unknown.join(", ") }) };
      else if (phraseCheck.wordCount !== 24) phraseStatus = { tone: "neutral", text: t("restore.phraseCount", { n: phraseCheck.wordCount }) };
      else phraseStatus = { tone: "bad", text: t("restore.phraseChecksum") };
    }
    return (
      <Screen
        title={t("restore.fromPhrase")}
        onBack={back.restorePhrase}
        footer={
          <Button
            variant="primary"
            loading={busy}
            disabled={!ready}
            onClick={async () => {
              const s = await run(() => api.setupRestorePhrase(phraseInput, password, kid));
              if (s) {
                setStarted(s);
                setSummary(null);
                setPhraseInput("");
                await startTotp();
              }
            }}
          >
            {t("restore.restorePhrase")}
          </Button>
        }
      >
        {progress}
        <div>
          <label htmlFor="phrase" className="mb-1.5 block px-1 text-[14px] font-bold text-ink">
            {t("restore.phraseLabel")}
          </label>
          <textarea
            id="phrase"
            rows={6}
            value={phraseInput}
            onChange={(e) => setPhraseInput(e.target.value)}
            autoCapitalize="off"
            autoCorrect="off"
            autoComplete="off"
            spellCheck={false}
            className="w-full rounded-input border-2 border-line-mid bg-white p-3.5 font-mono text-[16px] leading-relaxed text-ink outline-none focus:border-primary"
          />
          {phraseStatus && (
            <p
              className={`mt-1.5 px-1 text-[13.5px] font-semibold ${
                phraseStatus.tone === "ok" ? "text-success" : phraseStatus.tone === "bad" ? "text-danger" : "text-ink-soft"
              }`}
            >
              {phraseStatus.text}
            </p>
          )}
        </div>
        <Field
          label={t("restore.kidLabel")}
          value={kid}
          autoCapitalize="off"
          autoCorrect="off"
          spellCheck={false}
          onChange={(e) => setKid(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ""))}
        />
        <NewPasswordFields
          label={t("restore.newPassword")}
          password={password}
          confirm={confirm}
          onPassword={setPassword}
          onConfirm={setConfirm}
        />
        <ErrorLine message={error} />
      </Screen>
    );
  }

  if (step === "totpChoice" && summary) {
    return (
      <Screen title={t("setup.totp.keepTitle")} onBack={back.totpChoice}>
        {progress}
        <Banner tone="success">{t("restore.summary", { kid: summary.key.kid, count: summary.historyCount })}</Banner>
        <Card>
          <p className="text-[13.5px] text-ink-body">{t("restore.compare")}</p>
          <p className="mt-1 font-mono text-[18px] font-bold tracking-wider text-chrome">{summary.key.fingerprint}</p>
        </Card>
        <Lead>{t("setup.totp.keepBody")}</Lead>
        <CodeField label={t("setup.totp.codeLabel")} value={code} onChange={setCode} />
        <Button
          variant="primary"
          loading={busy}
          disabled={code.length !== 6}
          onClick={async () => {
            const ok = await run(async () => {
              await api.setupKeepTotp(code);
              return true;
            });
            if (ok) await commit();
          }}
        >
          {t("setup.totp.keep")}
        </Button>
        <Button variant="secondary" disabled={busy} onClick={() => void startTotp()}>
          {t("setup.totp.replace")}
        </Button>
        <ErrorLine message={error} />
      </Screen>
    );
  }

  if (step === "totp" && enrollment) {
    return (
      <Screen
        title={t("setup.totp.title")}
        onBack={back.totp}
        footer={
          <Button
            variant="primary"
            loading={busy}
            disabled={code.length !== 6}
            onClick={async () => {
              const ok = await run(async () => {
                await api.setupConfirmTotp(code);
                return true;
              });
              if (ok) await commit();
            }}
          >
            {t("setup.totp.confirm")}
          </Button>
        }
      >
        {progress}
        {mode === "restore" && started && !summary && (
          <Card>
            <p className="text-[13.5px] text-ink-body">{t("restore.compare")}</p>
            <p className="mt-1 font-mono text-[18px] font-bold tracking-wider text-chrome">{started.key.fingerprint}</p>
          </Card>
        )}
        <Lead>{t("setup.totp.body")}</Lead>
        <TotpEnroll enrollment={enrollment} code={code} onCode={setCode} error={error} />
      </Screen>
    );
  }

  // ── Post-commit (vault written, session unlocked) ──────────────────────────

  if (!sheet) {
    return (
      <Screen title={t("app.name")}>
        {error ? <ErrorLine message={error} /> : <p className="py-10 text-center text-[14px] font-semibold text-ink-soft">{t("app.working")}</p>}
      </Screen>
    );
  }

  if (step === "codes") {
    return (
      <Screen
        title={t("setup.codes.title")}
        footer={
          <Button variant="primary" disabled={!codesWritten} onClick={() => go("sheet")}>
            {t("app.next")}
          </Button>
        }
      >
        {progress}
        <Lead>{t("setup.codes.body")}</Lead>
        <CodeList codes={sheet.codes} />
        <Confirm checked={codesWritten} onChange={setCodesWritten}>
          {t("setup.codes.written")}
        </Confirm>
      </Screen>
    );
  }

  if (step === "sheet") {
    return (
      <Screen
        title={t("setup.sheet.title")}
        onBack={back.sheet}
        footer={
          <Button variant="primary" disabled={!sheetWritten} onClick={() => go("backup")}>
            {t("app.next")}
          </Button>
        }
      >
        {progress}
        <Lead>{t("setup.sheet.body")}</Lead>
        <Card className="space-y-4">
          <div className="flex items-center gap-3 border-b border-line pb-3">
            <span className="flex h-10 w-10 items-center justify-center rounded-xl bg-chrome text-mint-bright">
              <KeyIcon size={20} />
            </span>
            <div>
              <p className="text-[15px] font-extrabold text-ink">{t("app.name")}</p>
              <p className="text-[12.5px] text-ink-soft">{t("setup.sheet.title")}</p>
            </div>
          </div>
          <dl className="grid grid-cols-1 gap-2 text-[14px]">
            <div>
              <dt className="text-[12px] font-bold uppercase tracking-wider text-ink-soft">{t("setup.sheet.keyName")}</dt>
              <dd className="font-mono text-[16px] font-bold text-ink">{sheet.key.kid}</dd>
            </div>
            <div>
              <dt className="text-[12px] font-bold uppercase tracking-wider text-ink-soft">{t("setup.sheet.fingerprint")}</dt>
              <dd className="font-mono text-[16px] font-bold text-ink">{sheet.key.fingerprint}</dd>
            </div>
          </dl>
          <div>
            <p className="mb-2 text-[12px] font-bold uppercase tracking-wider text-ink-soft">{t("setup.sheet.words")}</p>
            <WordGrid words={sheet.phrase} />
          </div>
          <div>
            <p className="mb-2 text-[12px] font-bold uppercase tracking-wider text-ink-soft">{t("setup.sheet.codes")}</p>
            {codesKept || sheet.codes.length === 0 ? <Banner tone="info">{t("setup.codes.unchanged")}</Banner> : <CodeList codes={sheet.codes} />}
          </div>
        </Card>
        <Banner tone="warning">{t("setup.sheet.noScreenshot")}</Banner>
        <Confirm checked={sheetWritten} onChange={setSheetWritten}>
          {t("setup.sheet.written")}
        </Confirm>
      </Screen>
    );
  }

  if (step === "backup") {
    return (
      <Screen
        title={t("setup.backup.title")}
        onBack={back.backup}
        footer={
          <Button variant="primary" disabled={!backup || !backupSaved} onClick={() => go("publicKey")}>
            {t("app.next")}
          </Button>
        }
      >
        {progress}
        <Lead>{t("setup.backup.body")}</Lead>
        {!backup ? (
          <>
            <PasswordField label={t("setup.backup.reenter")} value={backupPassword} onChange={(e) => setBackupPassword(e.target.value)} />
            <Button
              variant="primary"
              loading={busy}
              disabled={backupPassword.length === 0}
              onClick={async () => {
                const file = await run(() => api.exportBackup(backupPassword));
                if (file) {
                  setBackup(file);
                  setBackupPassword("");
                }
              }}
            >
              {t("setup.backup.make")}
            </Button>
          </>
        ) : (
          <BackupFileActions file={backup} note={backupNote} setNote={setBackupNote} />
        )}
        <ErrorLine message={error} />
        {backup && (
          <Confirm checked={backupSaved} onChange={setBackupSaved}>
            {t("setup.backup.confirm")}
          </Confirm>
        )}
      </Screen>
    );
  }

  // publicKey
  return (
    <Screen
      title={t("setup.publicKey.title")}
      onBack={back.publicKey}
      footer={
        <Button
          variant="primary"
          loading={busy}
          onClick={async () => {
            const ok = await run(async () => {
              await api.finishSetup();
              return true;
            });
            if (ok) onFinished();
          }}
        >
          {t("setup.publicKey.finish")}
        </Button>
      }
    >
      {progress}
      <Lead>{t("setup.publicKey.body")}</Lead>
      <PublicKeyCard info={sheet.key} />
      <p className="px-1 text-[13px] leading-relaxed text-ink-soft">{t("setup.publicKey.fingerprintHint")}</p>
      <ErrorLine message={error} />
    </Screen>
  );
}

export function CodeList({ codes }: { codes: string[] }) {
  return (
    <ol className="grid grid-cols-2 gap-2">
      {codes.map((c, i) => (
        <li key={c} className="flex min-h-11 items-center gap-2 rounded-input border border-line-strong bg-white px-3">
          <span className="w-5 shrink-0 text-end font-mono text-[12px] font-bold text-ink-soft">{i + 1}</span>
          <span className="font-mono text-[15px] font-bold tracking-wide text-ink">{c}</span>
        </li>
      ))}
    </ol>
  );
}

/** "Send / upload" (share sheet) and "Save to a folder" for a backup file. */
export function BackupFileActions({ file, note, setNote }: { file: PreparedFile; note: string | null; setNote: (v: string | null) => void }) {
  const { t } = useI18n();
  return (
    <Card className="space-y-3">
      <Banner tone="success">{t("setup.backup.made", { name: file.fileName })}</Banner>
      <Button
        variant="primary"
        icon={<ShareIcon size={20} />}
        onClick={async () => {
          const outcome = await shareFile(file.path, "application/octet-stream", file.fileName);
          setNote(outcome === "unavailable" ? t("errors.share_unavailable") : null);
        }}
      >
        {t("setup.backup.shareFile")}
      </Button>
      <Button
        variant="secondary"
        icon={<DownloadIcon size={20} />}
        onClick={async () => {
          try {
            const saved = await saveTextFile(file.fileName, file.contents, "mfkbackup");
            setNote(saved ? t("result.saved") : null);
          } catch (e) {
            setNote(t("errors.save_failed", { message: String(e) }));
          }
        }}
      >
        {t("setup.backup.saveToPhone")}
      </Button>
      {note && <p className="px-1 text-[13.5px] font-semibold text-ink-body">{note}</p>}
    </Card>
  );
}
