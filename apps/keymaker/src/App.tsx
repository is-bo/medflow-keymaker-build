import { useCallback, useEffect, useRef, useState } from "react";
import { api, onLocked, type HistoryItem, type UnlockResult } from "./api";
import { useI18n } from "./i18n";
import { installBackButton } from "./nav";
import { isExternalActive } from "./platform";
import { Banner, Button, Screen } from "./ui/kit";
import { BackupScreen } from "./screens/BackupScreen";
import { History } from "./screens/History";
import { Home } from "./screens/Home";
import { IssueKey } from "./screens/IssueKey";
import { KeyView } from "./screens/KeyView";
import { SetupFlow } from "./screens/SetupFlow";
import { Settings } from "./screens/Settings";
import { Unlock } from "./screens/Unlock";
import { Welcome } from "./screens/Welcome";

type Top =
  | { kind: "boot" }
  | { kind: "bootError"; message: string }
  | { kind: "welcome" }
  | { kind: "setup"; mode: "new" | "restore" }
  | { kind: "locked"; wait: number }
  | { kind: "resume" }
  | { kind: "main"; notice: string | null };

// Mirrors core/src/engine.rs IDLE_MS / SETUP_IDLE_MS; Rust enforces both.
const IDLE_MS = 2 * 60_000;
const SETUP_IDLE_MS = 15 * 60_000;
const TOUCH_EVERY_MS = 30_000;

export function App() {
  const { t } = useI18n();
  const [top, setTop] = useState<Top>({ kind: "boot" });

  const refresh = useCallback(async () => {
    try {
      const status = await api.status();
      if (status.phase === "setup") setTop((prev) => (prev.kind === "setup" ? prev : { kind: "welcome" }));
      else if (status.phase === "locked") setTop({ kind: "locked", wait: status.waitSeconds });
      else setTop(status.setupFinished ? { kind: "main", notice: null } : { kind: "resume" });
    } catch (e) {
      setTop({ kind: "bootError", message: String((e as { message?: string }).message ?? e) });
    }
  }, []);

  useEffect(() => {
    void refresh();
    return installBackButton();
  }, [refresh]);

  // Rust said "locked" (idle timeout) → show the unlock screen.
  useEffect(() => onLocked(() => void refresh()), [refresh]);

  // ── Auto-lock: leaving the app, and inactivity ─────────────────────────────
  const sessionOpen = top.kind === "main" || top.kind === "resume" || top.kind === "setup";
  const lockNow = useCallback(async () => {
    await api.lock().catch(() => undefined);
    await refresh();
  }, [refresh]);

  const lastActivity = useRef(Date.now());
  const lastTouch = useRef(0);
  useEffect(() => {
    if (!sessionOpen) return;
    lastActivity.current = Date.now();
    const limit = top.kind === "main" ? IDLE_MS : SETUP_IDLE_MS;
    const onActivity = () => {
      lastActivity.current = Date.now();
      if (top.kind !== "setup" && Date.now() - lastTouch.current > TOUCH_EVERY_MS) {
        lastTouch.current = Date.now();
        void api.touch().catch(() => undefined);
      }
    };
    const onHidden = () => {
      if (document.visibilityState === "hidden" && !isExternalActive()) void lockNow();
    };
    const timer = window.setInterval(() => {
      if (Date.now() - lastActivity.current > limit && top.kind !== "setup") void lockNow();
    }, 5_000);
    const events = ["pointerdown", "keydown", "input", "scroll"] as const;
    events.forEach((name) => window.addEventListener(name, onActivity, { capture: true, passive: true }));
    document.addEventListener("visibilitychange", onHidden);
    window.addEventListener("pagehide", onHidden);
    return () => {
      window.clearInterval(timer);
      events.forEach((name) => window.removeEventListener(name, onActivity, { capture: true }));
      document.removeEventListener("visibilitychange", onHidden);
      window.removeEventListener("pagehide", onHidden);
    };
  }, [sessionOpen, top.kind, lockNow]);

  function afterUnlock(result: UnlockResult) {
    if (!result.setupFinished) return setTop({ kind: "resume" });
    setTop({ kind: "main", notice: result.usedRecoveryCode ? t("unlock.usedRecovery", { n: result.recoveryCodesLeft }) : null });
  }

  switch (top.kind) {
    case "boot":
      return <div className="h-full bg-chrome" aria-busy="true" />;
    case "bootError":
      return (
        <Screen title={t("app.name")}>
          <Banner tone="danger">{t("errors.generic", { message: top.message })}</Banner>
          <Button variant="primary" onClick={() => void refresh()}>
            {t("app.retry")}
          </Button>
        </Screen>
      );
    case "welcome":
      return <Welcome onNew={() => setTop({ kind: "setup", mode: "new" })} onRestore={() => setTop({ kind: "setup", mode: "restore" })} />;
    case "setup":
      return <SetupFlow mode={top.mode} onExit={() => setTop({ kind: "welcome" })} onFinished={() => setTop({ kind: "main", notice: null })} />;
    case "resume":
      return <SetupFlow mode="resume" onExit={() => void lockNow()} onFinished={() => setTop({ kind: "main", notice: null })} />;
    case "locked":
      return <Unlock initialWait={top.wait} onUnlocked={afterUnlock} onErased={() => setTop({ kind: "welcome" })} />;
    case "main":
      return <MainApp notice={top.notice} onLock={() => void lockNow()} />;
  }
}

type Route =
  | { name: "home" }
  | { name: "issue"; renewFrom?: HistoryItem }
  | { name: "key"; item: HistoryItem; fresh: boolean }
  | { name: "history" }
  | { name: "backup" }
  | { name: "settings" };

function MainApp({ notice, onLock }: { notice: string | null; onLock: () => void }) {
  const { t } = useI18n();
  const [stack, setStack] = useState<Route[]>([{ name: "home" }]);
  const [shownNotice, setShownNotice] = useState(notice);
  const route = stack[stack.length - 1] ?? { name: "home" };
  const push = (r: Route) => setStack((s) => [...s, r]);
  const pop = () => setStack((s) => (s.length > 1 ? s.slice(0, -1) : s));
  const home = () => setStack([{ name: "home" }]);

  switch (route.name) {
    case "issue":
      return (
        <IssueKey
          key={route.renewFrom?.licenseId ?? "new"}
          renewFrom={route.renewFrom}
          onBack={pop}
          onLock={onLock}
          // Replace the form with the result so Back from the result does not re-open a filled form.
          onIssued={(item) => setStack((s) => [...s.slice(0, -1), { name: "key", item, fresh: true }])}
        />
      );
    case "key":
      return (
        <KeyView
          item={route.item}
          fresh={route.fresh}
          onBack={pop}
          onLock={onLock}
          onDone={home}
          onRenew={() => push({ name: "issue", renewFrom: route.item })}
        />
      );
    case "history":
      return <History onBack={pop} onLock={onLock} onOpen={(item) => push({ name: "key", item, fresh: false })} />;
    case "backup":
      return <BackupScreen onBack={pop} onLock={onLock} />;
    case "settings":
      return <Settings onBack={pop} onLock={onLock} />;
    default:
      return (
        <>
          {shownNotice && (
            <div className="fixed inset-x-0 bottom-0 z-10 mx-auto max-w-md p-4 pb-[max(16px,env(safe-area-inset-bottom))]">
              <Banner tone="warning" action={<Button size="md" block={false} onClick={() => setShownNotice(null)}>{t("app.close")}</Button>}>
                {shownNotice}
              </Banner>
            </div>
          )}
          <Home
            onLock={onLock}
            onIssue={() => push({ name: "issue" })}
            onHistory={() => push({ name: "history" })}
            onBackup={() => push({ name: "backup" })}
            onSettings={() => push({ name: "settings" })}
            onOpenKey={(item) => push({ name: "key", item, fresh: false })}
          />
        </>
      );
  }
}
