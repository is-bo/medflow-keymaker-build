import { useEffect, useState } from "react";
import { api, type HistoryItem, type Status } from "../api";
import { useI18n } from "../i18n";
import { ChevronIcon, CloudIcon, HistoryIcon, PlusIcon, SettingsIcon } from "../ui/icons";
import { Banner, Button, Row, Screen, SectionTitle } from "../ui/kit";
import { KeyRow } from "./History";

export function Home({
  onLock,
  onIssue,
  onHistory,
  onBackup,
  onSettings,
  onOpenKey,
}: {
  onLock: () => void;
  onIssue: () => void;
  onHistory: () => void;
  onBackup: () => void;
  onSettings: () => void;
  onOpenKey: (item: HistoryItem) => void;
}) {
  const { t } = useI18n();
  const [status, setStatus] = useState<Status | null>(null);
  const [recent, setRecent] = useState<HistoryItem[] | null>(null);

  useEffect(() => {
    api.status().then(setStatus, () => undefined);
    api.history("").then((items) => setRecent(items.slice(0, 3)), () => setRecent([]));
  }, []);

  const since = status?.keysSinceBackup ?? 0;
  const neverBackedUp = status !== null && status.lastBackupAt === null;

  return (
    <Screen
      title={t("app.name")}
      onLock={onLock}
      actions={
        <button
          type="button"
          onClick={onSettings}
          aria-label={t("home.settings")}
          className="focus-ring flex h-12 w-12 items-center justify-center rounded-btn text-chrome-text active:bg-chrome-soft"
        >
          <SettingsIcon />
        </button>
      }
    >
      {(since > 0 || neverBackedUp) && (
        <Banner
          tone="warning"
          title={neverBackedUp ? t("home.neverBackedUp") : since === 1 ? t("home.backupReminderOne") : t("home.backupReminder", { n: since })}
          action={
            <Button variant="secondary" size="md" block={false} onClick={onBackup}>
              {t("home.backupNow")}
            </Button>
          }
        />
      )}

      <button
        type="button"
        onClick={onIssue}
        className="focus-ring flex min-h-[88px] w-full items-center gap-4 rounded-card bg-primary px-5 py-4 text-start text-white shadow-cta active:bg-primary-hover"
      >
        <span className="flex h-12 w-12 shrink-0 items-center justify-center rounded-2xl bg-white/15">
          <PlusIcon size={28} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-[19px] font-extrabold">{t("home.newKey")}</span>
          <span className="block text-[14px] font-semibold text-primary-pale">{t("home.newKeyHint")}</span>
        </span>
      </button>

      <div className="grid grid-cols-1 gap-2">
        <Row
          icon={<HistoryIcon />}
          title={t("home.history")}
          subtitle={status ? t("home.historyCount", { n: status.historyCount }) : undefined}
          onClick={onHistory}
          trailing={<ChevronIcon className="text-ink-faint" />}
        />
        <Row icon={<CloudIcon />} title={t("home.backup")} onClick={onBackup} trailing={<ChevronIcon className="text-ink-faint" />} />
      </div>

      <div className="flex items-end justify-between pt-2">
        <SectionTitle>{t("home.recent")}</SectionTitle>
        {recent && recent.length > 0 && (
          <button type="button" onClick={onHistory} className="min-h-10 px-1 text-[14px] font-bold text-primary">
            {t("home.seeAll")}
          </button>
        )}
      </div>
      {recent === null ? (
        <div className="h-20 animate-pulse rounded-card bg-surface-chip" />
      ) : recent.length === 0 ? (
        <p className="rounded-card border border-dashed border-line-mid px-4 py-6 text-center text-[14px] text-ink-soft">{t("home.empty")}</p>
      ) : (
        <div className="space-y-2">
          {recent.map((item) => (
            <KeyRow key={item.licenseId} item={item} onClick={() => onOpenKey(item)} />
          ))}
        </div>
      )}
    </Screen>
  );
}
