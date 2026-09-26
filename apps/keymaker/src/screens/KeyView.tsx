import { useState } from "react";
import { api, isKmError, type HistoryItem } from "../api";
import { useI18n } from "../i18n";
import { termLabel } from "../lib/logic";
import { copyText, saveTextFile, shareText } from "../platform";
import { CheckIcon, DownloadIcon, HistoryIcon, ShareIcon } from "../ui/icons";
import { Banner, Button, Card, CopyBox, Screen } from "../ui/kit";
import { StateBadge } from "./History";

/** A key just issued (`fresh`) or opened from the history. */
export function KeyView({
  item,
  fresh,
  onBack,
  onLock,
  onRenew,
  onDone,
}: {
  item: HistoryItem;
  fresh: boolean;
  onBack: () => void;
  onLock: () => void;
  onRenew: () => void;
  onDone: () => void;
}) {
  const { t, formatDate } = useI18n();
  const [note, setNote] = useState<{ tone: "success" | "warning"; text: string } | null>(null);

  async function share() {
    setNote(null);
    if ((await shareText(item.key)) === "unavailable") setNote({ tone: "warning", text: t("errors.share_unavailable") });
  }

  async function save() {
    setNote(null);
    try {
      const file = await api.keyFile(item.licenseId);
      if (await saveTextFile(file.fileName, file.contents, "mflic")) setNote({ tone: "success", text: t("result.saved") });
    } catch (e) {
      setNote({ tone: "warning", text: t("errors.save_failed", { message: isKmError(e) ? e.message : String(e) }) });
    }
  }

  const rows: [string, string][] = [
    [t("details.doctor"), item.licensee],
    ...(item.phone ? ([[t("details.phone"), item.phone]] as [string, string][]) : []),
    [t("details.machine"), item.machineCode],
    [t("details.duration"), termLabel(t, item.termKind, item.termCount)],
    [t("details.issued"), formatDate(item.issuedAt)],
    [t("details.expires"), item.expiresAt === null ? t("issue.never") : formatDate(item.expiresAt)],
    [t("details.features"), item.features.includes("telegram") ? "Telegram" : t("details.none")],
  ];

  return (
    <Screen
      title={fresh ? t("result.title") : item.licensee}
      onBack={onBack}
      onLock={onLock}
      footer={
        fresh ? (
          <Button variant="secondary" onClick={onDone}>
            {t("app.done")}
          </Button>
        ) : (
          <Button variant="secondary" icon={<HistoryIcon size={20} />} onClick={onRenew}>
            {t("history.renew")}
          </Button>
        )
      }
    >
      {fresh && (
        <div className="flex items-center gap-3 rounded-card bg-success-surface p-4">
          <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-success text-white">
            <CheckIcon size={24} />
          </span>
          <p className="text-[14.5px] leading-snug text-ink-body">{t("result.body")}</p>
        </div>
      )}

      <Card>
        <div className="mb-3 flex items-center justify-between gap-2">
          <p className="min-w-0 truncate text-[16px] font-extrabold text-ink">{item.licensee}</p>
          <StateBadge item={item} />
        </div>
        <dl className="divide-y divide-line">
          {rows.map(([label, value]) => (
            <div key={label} className="flex justify-between gap-4 py-2 text-[14px]">
              <dt className="shrink-0 text-ink-soft">{label}</dt>
              <dd className={`min-w-0 text-end font-semibold text-ink ${label === t("details.machine") ? "font-mono" : ""}`}>{value}</dd>
            </div>
          ))}
        </dl>
        {item.daysLeft !== null && item.state !== "expired" && (
          <p className="mt-2 text-[13px] font-semibold text-ink-soft">{t("status.daysLeft", { n: item.daysLeft })}</p>
        )}
      </Card>

      <Button variant="primary" icon={<ShareIcon size={20} />} onClick={() => void share()}>
        {fresh ? t("result.shareWhatsApp") : t("history.reshare")}
      </Button>
      <Button variant="secondary" icon={<DownloadIcon size={20} />} onClick={() => void save()}>
        {t("result.saveFile")}
      </Button>
      {note && <Banner tone={note.tone}>{note.text}</Banner>}

      <CopyBox label={t("result.keyLabel")} value={item.key} small onCopy={() => void copyText(item.key)} />

      <dl className="space-y-1 px-1 text-[12px] text-ink-soft">
        <div>
          {t("details.licenseId")}: <span className="font-mono">{item.licenseId}</span>
        </div>
        <div>
          {t("details.kid")}: <span className="font-mono">{item.kid}</span>
        </div>
        {item.renewalOf && <div>{t("details.renewalOf")}</div>}
      </dl>
    </Screen>
  );
}
