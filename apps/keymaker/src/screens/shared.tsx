import QRCode from "qrcode";
import { useEffect, useState } from "react";
import type { KeyInfo, TotpEnrollment } from "../api";
import { useI18n } from "../i18n";
import { passwordChecklist } from "../lib/logic";
import { copyText, holdForExternalApp, openAuthenticator, shareText } from "../platform";
import { Banner, Button, Card, CodeField, CopyBox, PasswordField } from "../ui/kit";
import { CheckIcon, KeyIcon, ShareIcon } from "../ui/icons";

/** New password + confirmation with a live rule checklist. */
export function NewPasswordFields({
  password,
  confirm,
  onPassword,
  onConfirm,
  label,
}: {
  password: string;
  confirm: string;
  onPassword: (v: string) => void;
  onConfirm: (v: string) => void;
  label?: string;
}) {
  const { t } = useI18n();
  const checks = passwordChecklist(password);
  const mismatch = confirm.length > 0 && confirm !== password;
  return (
    <div className="space-y-3">
      <PasswordField label={label ?? t("password.label")} value={password} onChange={(e) => onPassword(e.target.value)} />
      <ul className="grid grid-cols-1 gap-1 px-1">
        {checks.map(({ rule, ok }) => (
          <li key={rule} className={`flex items-center gap-2 text-[13.5px] font-semibold ${ok ? "text-success" : "text-ink-soft"}`}>
            <span className={`flex h-5 w-5 items-center justify-center rounded-full ${ok ? "bg-success text-white" : "border-2 border-line-mid"}`}>
              {ok && <CheckIcon size={13} />}
            </span>
            {t(`password.rules.${rule}`)}
          </li>
        ))}
      </ul>
      <PasswordField
        label={t("password.confirm")}
        value={confirm}
        onChange={(e) => onConfirm(e.target.value)}
        error={mismatch ? t("password.mismatch") : null}
      />
    </div>
  );
}

export function passwordReady(password: string, confirm: string): boolean {
  return password === confirm && passwordChecklist(password).every((c) => c.ok);
}

/**
 * Authenticator enrollment: QR (for a second phone), "open in authenticator"
 * (same phone — you cannot scan your own screen), and the secret to type.
 * While mounted, switching to the authenticator app does not lock Key Maker.
 */
export function TotpEnroll({
  enrollment,
  code,
  onCode,
  error,
}: {
  enrollment: TotpEnrollment;
  code: string;
  onCode: (v: string) => void;
  error?: string | null;
}) {
  const { t } = useI18n();
  const [qr, setQr] = useState<string | null>(null);
  useEffect(() => holdForExternalApp(), []);
  useEffect(() => {
    let alive = true;
    QRCode.toDataURL(enrollment.uri, { margin: 1, width: 480, errorCorrectionLevel: "M", color: { dark: "#0e343f" } })
      .then((url) => alive && setQr(url))
      .catch(() => alive && setQr(null));
    return () => {
      alive = false;
    };
  }, [enrollment.uri]);

  return (
    <div className="space-y-4">
      <Card className="space-y-4">
        <p className="text-[14px] leading-relaxed text-ink-body">{t("setup.totp.otherPhone")}</p>
        <div className="mx-auto w-56 rounded-2xl border border-line-strong bg-white p-2">
          {qr ? <img src={qr} alt="" className="h-auto w-full" /> : <div className="aspect-square w-full animate-pulse rounded-xl bg-surface-chip" />}
        </div>
        <p className="text-[14px] leading-relaxed text-ink-body">{t("setup.totp.samePhone")}</p>
        <Button variant="secondary" icon={<KeyIcon size={20} />} onClick={() => void openAuthenticator(enrollment.uri)}>
          {t("setup.totp.openApp")}
        </Button>
        <dl className="space-y-2 rounded-input bg-surface-soft p-3 text-[13.5px]">
          <div className="flex justify-between gap-3">
            <dt className="text-ink-soft">{t("setup.totp.issuer")}</dt>
            <dd className="font-semibold text-ink">MedFlow Key Maker</dd>
          </div>
          <div className="flex justify-between gap-3">
            <dt className="text-ink-soft">{t("setup.totp.account")}</dt>
            <dd className="font-mono font-semibold text-ink">{enrollment.account}</dd>
          </div>
          <div>
            <dt className="text-ink-soft">{t("setup.totp.secret")}</dt>
            <dd className="mt-1 select-text break-all font-mono text-[16px] font-bold tracking-wide text-ink">{enrollment.secret}</dd>
          </div>
        </dl>
      </Card>
      <CodeField label={t("setup.totp.codeLabel")} value={code} onChange={onCode} error={error} />
    </div>
  );
}

/** The public-key row with copy / share and its fingerprint. */
export function PublicKeyCard({ info }: { info: KeyInfo }) {
  const { t } = useI18n();
  const [shareError, setShareError] = useState(false);
  return (
    <Card className="space-y-4">
      <CopyBox label={`kid: ${info.kid}`} value={info.row} small onCopy={() => void copyText(info.row)} />
      <div className="rounded-input bg-primary-surface px-3 py-2.5">
        <p className="text-[12px] font-bold uppercase tracking-wider text-primary">{t("setup.sheet.fingerprint")}</p>
        <p className="mt-0.5 font-mono text-[18px] font-bold tracking-wider text-chrome">{info.fingerprint}</p>
      </div>
      <Button
        variant="secondary"
        icon={<ShareIcon size={20} />}
        onClick={async () => setShareError((await shareText(info.row)) === "unavailable")}
      >
        {t("app.share")}
      </Button>
      {shareError && <Banner tone="warning">{t("errors.share_unavailable")}</Banner>}
    </Card>
  );
}
