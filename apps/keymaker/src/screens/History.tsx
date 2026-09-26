import { useEffect, useState } from "react";
import { api, type HistoryItem, type KeyState } from "../api";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { termLabel } from "../lib/logic";
import { saveTextFile, shareFile } from "../platform";
import { DownloadIcon, SearchIcon, ShareIcon } from "../ui/icons";
import { Badge, Button, ErrorLine, Screen } from "../ui/kit";

const STATE_TONE: Record<KeyState, "primary" | "success" | "warning" | "danger"> = {
  lifetime: "primary",
  active: "success",
  expiring: "warning",
  expired: "danger",
};

export function StateBadge({ item }: { item: HistoryItem }) {
  const { t } = useI18n();
  return <Badge tone={STATE_TONE[item.state]}>{t(`status.${item.state}`)}</Badge>;
}

export function KeyRow({ item, onClick }: { item: HistoryItem; onClick: () => void }) {
  const { t, formatDate } = useI18n();
  return (
    <button
      type="button"
      onClick={onClick}
      className="focus-ring block w-full rounded-card border border-line bg-white px-4 py-3 text-start shadow-card active:bg-surface-soft"
    >
      <div className="flex items-start justify-between gap-2">
        <span className="min-w-0 truncate text-[15.5px] font-bold text-ink">{item.licensee}</span>
        <StateBadge item={item} />
      </div>
      <p className="mt-0.5 font-mono text-[13px] text-ink-mid">{item.machineCode}</p>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-[13px] text-ink-soft">
        <span>{termLabel(t, item.termKind, item.termCount)}</span>
        <span aria-hidden="true">·</span>
        <span>{item.expiresAt === null ? t("issue.never") : t("issue.expires", { date: formatDate(item.expiresAt) })}</span>
        {item.features.includes("telegram") && <Badge tone="violet">Telegram</Badge>}
      </div>
    </button>
  );
}

export function History({ onBack, onLock, onOpen }: { onBack: () => void; onLock: () => void; onOpen: (item: HistoryItem) => void }) {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<HistoryItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      api.history(query).then(
        (list) => {
          setItems(list);
          setError(null);
        },
        (e) => setError(errorText(t, e)),
      );
    }, 150);
    return () => window.clearTimeout(timer);
  }, [query, t]);

  async function exportCsv(mode: "share" | "save") {
    setNote(null);
    try {
      const file = await api.historyCsvFile();
      if (mode === "share") {
        if ((await shareFile(file.path, "text/csv", file.fileName)) === "unavailable") setNote(t("errors.share_unavailable"));
      } else if (await saveTextFile(file.fileName, file.contents, "csv")) {
        setNote(t("result.saved"));
      }
    } catch (e) {
      setNote(errorText(t, e));
    }
  }

  return (
    <Screen title={t("history.title")} onBack={onBack} onLock={onLock}>
      <label className="flex min-h-[52px] items-center gap-2 rounded-input border-2 border-line-mid bg-white px-3.5 focus-within:border-primary">
        <SearchIcon size={20} className="shrink-0 text-ink-soft" />
        <span className="sr-only">{t("history.search")}</span>
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("history.search")}
          autoComplete="off"
          autoCorrect="off"
          className="min-w-0 flex-1 bg-transparent text-[16px] text-ink outline-none placeholder:text-ink-faint"
        />
      </label>

      <ErrorLine message={error} />
      {items === null && !error ? (
        <div className="space-y-2">
          {[0, 1, 2].map((i) => (
            <div key={i} className="h-24 animate-pulse rounded-card bg-surface-chip" />
          ))}
        </div>
      ) : items && items.length === 0 ? (
        <p className="rounded-card border border-dashed border-line-mid px-4 py-8 text-center text-[14px] text-ink-soft">
          {query.trim() ? t("history.noMatch", { q: query.trim() }) : t("history.empty")}
        </p>
      ) : (
        <div className="space-y-2">
          {items?.map((item) => (
            <KeyRow key={item.licenseId} item={item} onClick={() => onOpen(item)} />
          ))}
        </div>
      )}

      {items && items.length > 0 && !query.trim() && (
        <div className="grid grid-cols-2 gap-2 pt-2">
          <Button variant="secondary" size="md" icon={<ShareIcon size={18} />} onClick={() => void exportCsv("share")}>
            {t("history.exportCsv")}
          </Button>
          <Button variant="secondary" size="md" icon={<DownloadIcon size={18} />} onClick={() => void exportCsv("save")}>
            {t("setup.backup.saveToPhone")}
          </Button>
        </div>
      )}
      {note && <p className="px-1 text-[13.5px] font-semibold text-ink-body">{note}</p>}
    </Screen>
  );
}
