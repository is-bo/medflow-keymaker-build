import { useEffect, useState } from "react";
import { api, type PreparedFile, type Status } from "../api";
import { useI18n } from "../i18n";
import { errorText } from "../lib/errors";
import { Banner, Button, Card, ErrorLine, Lead, PasswordField, Screen } from "../ui/kit";
import { BackupFileActions } from "./SetupFlow";

export function BackupScreen({ onBack, onLock }: { onBack: () => void; onLock: () => void }) {
  const { t, formatDate } = useI18n();
  const [status, setStatus] = useState<Status | null>(null);
  const [password, setPassword] = useState("");
  const [file, setFile] = useState<PreparedFile | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    api.status().then(setStatus, () => undefined);
  }, [file]);

  return (
    <Screen title={t("backup.title")} onBack={onBack} onLock={onLock}>
      {status && (
        <Card className="space-y-1">
          <p className="text-[15px] font-bold text-ink">
            {status.lastBackupAt === null ? t("backup.never") : t("backup.last", { date: formatDate(status.lastBackupAt) })}
          </p>
          {status.lastBackupAt !== null && <p className="text-[14px] text-ink-body">{t("backup.since", { n: status.keysSinceBackup })}</p>}
        </Card>
      )}
      {status && (status.lastBackupAt === null || status.keysSinceBackup > 0) && !file && (
        <Banner tone="warning">{status.lastBackupAt === null ? t("home.neverBackedUp") : t("home.backupReminder", { n: status.keysSinceBackup })}</Banner>
      )}
      <Lead>{t("backup.what")}</Lead>
      {!file ? (
        <>
          <PasswordField label={t("setup.backup.reenter")} value={password} onChange={(e) => setPassword(e.target.value)} />
          <Button
            variant="primary"
            loading={busy}
            disabled={!password}
            onClick={async () => {
              setBusy(true);
              setError(null);
              try {
                setFile(await api.exportBackup(password));
                setPassword("");
              } catch (e) {
                setError(errorText(t, e));
              } finally {
                setBusy(false);
              }
            }}
          >
            {t("setup.backup.make")}
          </Button>
        </>
      ) : (
        <BackupFileActions file={file} note={note} setNote={setNote} />
      )}
      <ErrorLine message={error} />
    </Screen>
  );
}
