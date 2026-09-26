import { useEffect, useState } from "react";
import { api, type HistoryItem, type MachineCheck } from "../api";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { choiceForTerm, DURATION_CHOICES, termFor, type CustomUnit, type DurationChoice } from "../lib/logic";
import { CheckIcon } from "../ui/icons";
import { Banner, Button, Card, ErrorLine, Field, Screen } from "../ui/kit";

/**
 * The key form. Machine codes are validated live by Rust (MachineCode::parse,
 * checksum included) so a typo is caught before anything is signed; the
 * expiry preview uses the same calendar code that signs the key.
 */
export function IssueKey({
  renewFrom,
  onBack,
  onLock,
  onIssued,
}: {
  renewFrom?: HistoryItem;
  onBack: () => void;
  onLock: () => void;
  onIssued: (item: HistoryItem) => void;
}) {
  const { t, formatDate } = useI18n();
  const preset = renewFrom ? choiceForTerm(renewFrom.termKind, renewFrom.termCount) : null;
  const [licensee, setLicensee] = useState(renewFrom?.licensee ?? "");
  const [phone, setPhone] = useState(renewFrom?.phone ?? "");
  const [machine, setMachine] = useState(renewFrom?.machineCode ?? "");
  const [check, setCheck] = useState<MachineCheck | null>(null);
  const [touched, setTouched] = useState(Boolean(renewFrom));
  const [choice, setChoice] = useState<DurationChoice>(preset?.choice ?? "y1");
  const [customCount, setCustomCount] = useState(preset?.customCount ?? "");
  const [customUnit, setCustomUnit] = useState<CustomUnit>(preset?.customUnit ?? "months");
  const [telegram, setTelegram] = useState(renewFrom?.features.includes("telegram") ?? false);
  const [expiry, setExpiry] = useState<number | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const term = termFor(choice, customCount, customUnit);

  useEffect(() => {
    if (!machine.trim()) {
      setCheck(null);
      return;
    }
    let alive = true;
    api.checkMachineCode(machine).then((c) => alive && setCheck(c), () => undefined);
    return () => {
      alive = false;
    };
  }, [machine]);

  useEffect(() => {
    if (!term) {
      setExpiry(undefined);
      return;
    }
    let alive = true;
    api.previewExpiry(term).then((e) => alive && setExpiry(e), () => alive && setExpiry(undefined));
    return () => {
      alive = false;
    };
    // term is derived from these three
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [choice, customCount, customUnit]);

  let machineError: string | null = null;
  if (touched && check && !check.ok && check.error) {
    machineError = t(`issue.machineErrors.${check.error}`, {
      n: check.symbols ?? 0,
      position: check.position ?? 0,
      character: check.character ?? "",
    });
  }

  const ready = licensee.trim().length > 0 && check?.ok === true && term !== null;
  const lifetimeRenewal = renewFrom?.expiresAt === null && choice !== "lifetime";

  async function submit() {
    if (!term || !check?.canonical) return;
    setBusy(true);
    setError(null);
    try {
      const item = await api.issueKey({
        licensee: licensee.trim(),
        phone: phone.trim(),
        machineCode: check.canonical,
        term,
        telegram,
        renewalOf: renewFrom?.licenseId ?? null,
      });
      onIssued(item);
    } catch (e) {
      setError(errorText(t, e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Screen
      title={renewFrom ? t("issue.renewTitle") : t("issue.title")}
      onBack={onBack}
      onLock={onLock}
      footer={
        <Button variant="primary" loading={busy} disabled={!ready} onClick={() => void submit()}>
          {busy ? t("issue.creating") : t("issue.create")}
        </Button>
      }
    >
      {renewFrom && <Banner tone="info">{t("issue.renewing", { name: renewFrom.licensee })}</Banner>}

      <Field
        label={t("issue.doctor")}
        value={licensee}
        onChange={(e) => setLicensee(e.target.value)}
        autoCapitalize="words"
        maxLength={120}
      />
      <Field
        label={t("issue.phone")}
        type="tel"
        inputMode="tel"
        value={phone}
        onChange={(e) => setPhone(e.target.value.replace(/[^0-9 +\-().\/]/g, ""))}
        maxLength={40}
      />
      <Field
        label={t("issue.machine")}
        value={machine}
        onChange={(e) => setMachine(e.target.value.toUpperCase())}
        onBlur={() => setTouched(true)}
        placeholder="MF-XXXX-XXXX-XXXX-XXXX"
        autoCapitalize="characters"
        autoCorrect="off"
        autoComplete="off"
        spellCheck={false}
        className="[&_input]:font-mono [&_input]:tracking-wide"
        hint={t("issue.machineHint")}
        error={machineError}
        ok={check?.ok ? `${t("issue.machineOk")} · ${check.canonical}` : null}
      />

      <fieldset>
        <legend className="mb-2 px-1 text-[14px] font-bold text-ink">{t("issue.duration")}</legend>
        <div className="grid grid-cols-3 gap-2">
          {DURATION_CHOICES.map((c) => (
            <button
              key={c}
              type="button"
              aria-pressed={choice === c}
              onClick={() => setChoice(c)}
              className={`focus-ring min-h-12 rounded-btn border-2 px-2 text-[14.5px] font-bold transition-colors ${
                choice === c ? "border-primary bg-primary text-white" : "border-line-mid bg-white text-ink-mid active:bg-primary-wash"
              } ${c === "custom" ? "col-span-2" : ""}`}
            >
              {t(`issue.durations.${c}`)}
            </button>
          ))}
        </div>
        {choice === "custom" && (
          <Card className="mt-3 grid grid-cols-2 gap-3 p-3">
            <Field
              label={t("issue.customCount")}
              value={customCount}
              inputMode="numeric"
              onChange={(e) => setCustomCount(e.target.value.replace(/\D/g, "").slice(0, 4))}
            />
            <div>
              <p className="mb-1.5 px-1 text-[14px] font-bold text-ink">&nbsp;</p>
              <div role="group" className="grid h-[52px] grid-cols-2 gap-1 rounded-input border-2 border-line-mid bg-white p-1">
                {(["months", "days"] as const).map((u) => (
                  <button
                    key={u}
                    type="button"
                    aria-pressed={customUnit === u}
                    onClick={() => setCustomUnit(u)}
                    className={`rounded-lg text-[14px] font-bold ${customUnit === u ? "bg-primary text-white" : "text-ink-mid"}`}
                  >
                    {t(`issue.${u}`)}
                  </button>
                ))}
              </div>
            </div>
          </Card>
        )}
        <p className="mt-2 px-1 text-[14px] font-semibold text-ink-body" aria-live="polite">
          {expiry === null ? t("issue.never") : typeof expiry === "number" ? t("issue.expires", { date: formatDate(expiry) }) : " "}
        </p>
      </fieldset>

      <label
        className={`flex min-h-16 cursor-pointer items-center gap-3 rounded-card border-2 px-4 py-3 ${
          telegram ? "border-violet bg-violet-surface" : "border-line-mid bg-white"
        }`}
      >
        <input type="checkbox" className="sr-only" checked={telegram} onChange={(e) => setTelegram(e.target.checked)} />
        <span
          aria-hidden="true"
          className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border-2 ${
            telegram ? "border-violet bg-violet text-white" : "border-line-mid"
          }`}
        >
          {telegram && <CheckIcon size={18} />}
        </span>
        <span>
          <span className="block text-[15.5px] font-bold text-ink">{t("issue.telegram")}</span>
          <span className="block text-[13px] text-ink-soft">{t("issue.telegramHint")}</span>
        </span>
      </label>

      {lifetimeRenewal && <Banner tone="warning">{t("issue.lifetimeNote")}</Banner>}
      <ErrorLine message={error} />
    </Screen>
  );
}
