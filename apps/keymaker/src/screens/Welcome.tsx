import { useI18n } from "../i18n";
import { CloudIcon, KeyIcon } from "../ui/icons";
import { LanguageSwitch, Row } from "../ui/kit";

export function Welcome({ onNew, onRestore }: { onNew: () => void; onRestore: () => void }) {
  const { t } = useI18n();
  return (
    <div className="flex h-full flex-col bg-chrome">
      <div className="mx-auto flex w-full max-w-md flex-1 flex-col px-5 pb-[max(20px,env(safe-area-inset-bottom))] pt-[max(20px,env(safe-area-inset-top))]">
        <div className="flex justify-end">
          <LanguageSwitch />
        </div>
        <div className="mt-10 flex flex-1 flex-col">
          <span className="flex h-16 w-16 items-center justify-center rounded-2xl bg-chrome-soft text-mint-bright">
            <KeyIcon size={34} />
          </span>
          <h1 className="mt-6 text-[28px] font-extrabold leading-tight tracking-tight text-white">{t("app.name")}</h1>
          <p className="mt-3 text-[15.5px] leading-relaxed text-chrome-text">{t("welcome.body")}</p>
          <p className="mt-3 text-[13.5px] font-semibold text-chrome-faint">{t("app.tagline")}</p>
        </div>
        <div className="space-y-3">
          <button
            type="button"
            onClick={onNew}
            className="focus-ring flex min-h-[72px] w-full items-center gap-4 rounded-card bg-mint-bright px-5 py-4 text-start text-chrome active:opacity-90"
          >
            <KeyIcon size={26} />
            <span>
              <span className="block text-[17px] font-extrabold">{t("welcome.newSetup")}</span>
              <span className="block text-[13.5px] font-semibold opacity-80">{t("welcome.newSetupHint")}</span>
            </span>
          </button>
          <Row icon={<CloudIcon />} title={t("welcome.restore")} subtitle={t("welcome.restoreHint")} onClick={onRestore} />
        </div>
      </div>
    </div>
  );
}
