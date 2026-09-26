import { useId, useState, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";
import { useI18n } from "../i18n";
import { useBackHandler } from "../nav";
import { AlertIcon, BackIcon, CheckIcon, CopyIcon, EyeIcon, LockIcon } from "./icons";

/**
 * Key Maker's UI kit: the desktop's tokens and button vocabulary
 * (apps/desktop/src/components/ui), scaled for a phone held in one hand —
 * 48–56px touch targets, 16px input text (no zoom-on-focus), sticky bottom
 * actions within thumb reach.
 */

type Variant = "primary" | "secondary" | "ghost" | "danger" | "mint";

const VARIANTS: Record<Variant, string> = {
  primary: "bg-primary text-white shadow-cta active:bg-primary-hover border border-transparent",
  secondary: "bg-white text-primary-hover border border-line-mid active:bg-primary-wash",
  ghost: "bg-transparent text-primary border border-transparent active:bg-primary-surface",
  danger: "bg-white text-danger border border-danger-border active:bg-danger-surface",
  mint: "bg-mint-bright text-chrome border border-transparent active:opacity-90",
};

export function Button({
  variant = "secondary",
  size = "lg",
  icon,
  loading = false,
  block = true,
  className = "",
  children,
  disabled,
  type = "button",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant;
  size?: "md" | "lg";
  icon?: ReactNode;
  loading?: boolean;
  block?: boolean;
}) {
  const sizing = size === "lg" ? "min-h-14 px-5 text-[16px]" : "min-h-12 px-4 text-[15px]";
  return (
    <button
      type={type}
      aria-busy={loading || undefined}
      disabled={disabled || loading}
      className={
        "focus-ring relative inline-flex items-center justify-center gap-2 rounded-btn font-bold leading-tight transition-colors " +
        "disabled:cursor-not-allowed disabled:opacity-50 disabled:shadow-none " +
        `${block ? "w-full" : ""} ${sizing} ${VARIANTS[variant]} ${className}`
      }
      {...rest}
    >
      <span className={`inline-flex items-center gap-2 ${loading ? "invisible" : ""}`}>
        {icon}
        {children}
      </span>
      {loading && (
        <span className="absolute inset-0 flex items-center justify-center">
          <Spinner />
        </span>
      )}
    </button>
  );
}

export function Spinner({ className = "h-5 w-5" }: { className?: string }) {
  return (
    <svg className={`animate-spin ${className}`} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="3" opacity="0.25" />
      <path d="M21 12a9 9 0 0 0-9-9" stroke="currentColor" strokeWidth="3" strokeLinecap="round" />
    </svg>
  );
}

/** App bar + scrolling body + optional sticky footer for the main action. */
export function Screen({
  title,
  onBack,
  onLock,
  actions,
  footer,
  children,
}: {
  title: string;
  onBack?: () => void;
  onLock?: () => void;
  actions?: ReactNode;
  footer?: ReactNode;
  children: ReactNode;
}) {
  const { t } = useI18n();
  useBackHandler(onBack);
  return (
    <div className="flex h-full flex-col bg-surface-app">
      <header className="shrink-0 bg-chrome pt-[env(safe-area-inset-top)] text-white">
        <div className="mx-auto flex h-14 w-full max-w-md items-center gap-1 px-2">
          {onBack ? (
            <button
              type="button"
              onClick={onBack}
              aria-label={t("app.back")}
              className="focus-ring flex h-12 w-12 items-center justify-center rounded-btn text-chrome-text active:bg-chrome-soft"
            >
              <BackIcon />
            </button>
          ) : (
            <span className="w-3" />
          )}
          <h1 className="min-w-0 flex-1 truncate text-[17px] font-extrabold tracking-tight">{title}</h1>
          {actions}
          {onLock && (
            <button
              type="button"
              onClick={onLock}
              aria-label={t("app.lock")}
              title={t("app.lock")}
              className="focus-ring flex h-12 w-12 items-center justify-center rounded-btn text-mint-bright active:bg-chrome-soft"
            >
              <LockIcon />
            </button>
          )}
        </div>
      </header>
      <main className="min-h-0 flex-1 overflow-y-auto overscroll-contain">
        <div className="mx-auto w-full max-w-md animate-mffade space-y-4 px-4 pb-8 pt-5">{children}</div>
      </main>
      {footer && (
        <footer className="shrink-0 border-t border-line-strong bg-white shadow-sheet">
          <div className="mx-auto w-full max-w-md space-y-2 px-4 pb-[max(12px,env(safe-area-inset-bottom))] pt-3">{footer}</div>
        </footer>
      )}
    </div>
  );
}

export function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <div className={`rounded-card border border-line bg-white p-4 shadow-card ${className}`}>{children}</div>;
}

export function Lead({ children }: { children: ReactNode }) {
  return <p className="text-[15px] leading-relaxed text-ink-body">{children}</p>;
}

export function SectionTitle({ children }: { children: ReactNode }) {
  return <h2 className="px-1 text-[12px] font-bold uppercase tracking-wider text-ink-soft">{children}</h2>;
}

type Tone = "info" | "warning" | "danger" | "success";
const BANNER: Record<Tone, string> = {
  info: "border-primary-border bg-primary-surface text-chrome-soft",
  warning: "border-warning-border bg-warning-surface text-warning",
  danger: "border-danger-border bg-danger-surface text-danger",
  success: "border-success-border bg-success-surface text-success",
};

export function Banner({ tone = "info", title, children, action }: { tone?: Tone; title?: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div role={tone === "danger" ? "alert" : "status"} className={`flex gap-3 rounded-card border p-3.5 ${BANNER[tone]}`}>
      <span className="mt-0.5 shrink-0">{tone === "success" ? <CheckIcon size={20} /> : <AlertIcon size={20} />}</span>
      <div className="min-w-0 flex-1 text-[14px] leading-relaxed">
        {title && <p className="font-extrabold">{title}</p>}
        {children && <div className={title ? "mt-0.5" : ""}>{children}</div>}
        {action && <div className="mt-2">{action}</div>}
      </div>
    </div>
  );
}

export function ErrorLine({ message }: { message: string | null | undefined }) {
  if (!message) return null;
  return <Banner tone="danger">{message}</Banner>;
}

export function Field({
  label,
  hint,
  error,
  ok,
  trailing,
  className = "",
  ...input
}: InputHTMLAttributes<HTMLInputElement> & {
  label: string;
  hint?: ReactNode;
  error?: string | null;
  ok?: string | null;
  trailing?: ReactNode;
}) {
  const id = useId();
  const describedBy = `${id}-desc`;
  const border = error ? "border-danger" : ok ? "border-success" : "border-line-mid focus-within:border-primary";
  return (
    <div className={className}>
      <label htmlFor={id} className="mb-1.5 block px-1 text-[14px] font-bold text-ink">
        {label}
      </label>
      <div className={`flex items-center rounded-input border-2 bg-white transition-colors ${border}`}>
        <input
          id={id}
          aria-invalid={error ? true : undefined}
          aria-describedby={describedBy}
          className="min-h-[52px] w-full min-w-0 flex-1 rounded-input bg-transparent px-3.5 text-[16px] text-ink outline-none placeholder:text-ink-faint"
          {...input}
        />
        {trailing}
      </div>
      <div id={describedBy} className="px-1">
        {error ? (
          <p className="mt-1.5 text-[13.5px] font-semibold text-danger">{error}</p>
        ) : ok ? (
          <p className="mt-1.5 flex items-center gap-1 text-[13.5px] font-semibold text-success">
            <CheckIcon size={16} /> {ok}
          </p>
        ) : hint ? (
          <p className="mt-1.5 text-[13px] leading-snug text-ink-soft">{hint}</p>
        ) : null}
      </div>
    </div>
  );
}

export function PasswordField(props: Omit<Parameters<typeof Field>[0], "type" | "trailing">) {
  const { t } = useI18n();
  const [visible, setVisible] = useState(false);
  return (
    <Field
      {...props}
      type={visible ? "text" : "password"}
      autoComplete="off"
      autoCapitalize="off"
      autoCorrect="off"
      spellCheck={false}
      trailing={
        <button
          type="button"
          onClick={() => setVisible((v) => !v)}
          aria-pressed={visible}
          aria-label={visible ? t("password.hide") : t("password.show")}
          className="focus-ring me-1 flex h-11 w-11 shrink-0 items-center justify-center rounded-btn text-ink-soft active:bg-surface-chip"
        >
          <EyeIcon size={20} />
        </button>
      }
    />
  );
}

/** Large one-time-code input: digits only, spaced, numeric keyboard. */
export function CodeField({ label, value, onChange, error }: { label: string; value: string; onChange: (v: string) => void; error?: string | null }) {
  return (
    <Field
      label={label}
      value={value}
      onChange={(e) => onChange(e.target.value.replace(/\D/g, "").slice(0, 6))}
      inputMode="numeric"
      autoComplete="one-time-code"
      pattern="[0-9]*"
      maxLength={6}
      placeholder="000000"
      error={error}
      className="[&_input]:text-center [&_input]:font-mono [&_input]:text-[24px] [&_input]:tracking-[0.4em]"
    />
  );
}

/** Big tappable confirmation row (not a tiny checkbox). */
export function Confirm({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children: ReactNode }) {
  return (
    <label
      className={`flex min-h-14 cursor-pointer items-center gap-3 rounded-card border-2 px-4 py-3 transition-colors ${
        checked ? "border-success bg-success-surface" : "border-line-mid bg-white"
      }`}
    >
      <input type="checkbox" className="sr-only" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span
        aria-hidden="true"
        className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border-2 ${
          checked ? "border-success bg-success text-white" : "border-line-mid bg-white"
        }`}
      >
        {checked && <CheckIcon size={18} />}
      </span>
      <span className="text-[15px] font-semibold leading-snug text-ink">{children}</span>
    </label>
  );
}

export function Badge({ tone, children }: { tone: "primary" | "success" | "warning" | "danger" | "violet" | "neutral"; children: ReactNode }) {
  const tones = {
    primary: "bg-primary-surface text-primary",
    success: "bg-success-surface text-success",
    warning: "bg-warning-surface text-warning",
    danger: "bg-danger-surface text-danger",
    violet: "bg-violet-surface text-violet",
    neutral: "bg-surface-chip text-ink-mid",
  } as const;
  return <span className={`inline-flex items-center rounded-full px-2.5 py-1 text-[12px] font-bold ${tones[tone]}`}>{children}</span>;
}

/** Monospace box for keys / the public-key row, with a copy button. */
export function CopyBox({ label, value, onCopy, small = false }: { label: string; value: string; onCopy?: () => void; small?: boolean }) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  return (
    <div>
      <p className="mb-1.5 px-1 text-[14px] font-bold text-ink">{label}</p>
      <div className="rounded-input border border-line-strong bg-surface-soft p-3">
        <p className={`select-text break-all font-mono leading-relaxed text-ink ${small ? "text-[11.5px]" : "text-[13px]"}`}>{value}</p>
        {onCopy && (
          <button
            type="button"
            onClick={() => {
              onCopy();
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1800);
            }}
            className="focus-ring mt-2 inline-flex min-h-11 items-center gap-2 rounded-btn px-2 text-[14px] font-bold text-primary active:bg-primary-surface"
          >
            {copied ? <CheckIcon size={18} /> : <CopyIcon size={18} />}
            {copied ? t("app.copied") : t("app.copy")}
          </button>
        )}
      </div>
    </div>
  );
}

/** Numbered 2-column word grid for the recovery phrase. */
export function WordGrid({ words }: { words: string[] }) {
  return (
    <ol className="grid grid-cols-2 gap-2">
      {words.map((word, i) => (
        <li key={i} className="flex min-h-11 items-center gap-2 rounded-input border border-line-strong bg-white px-3">
          <span className="w-6 shrink-0 text-end font-mono text-[12px] font-bold text-ink-soft">{i + 1}</span>
          <span className="font-mono text-[16px] font-semibold text-ink">{word}</span>
        </li>
      ))}
    </ol>
  );
}

export function Steps({ current, total }: { current: number; total: number }) {
  const { t } = useI18n();
  return (
    <div className="flex items-center gap-3 px-1" aria-label={t("setup.stepOf", { n: current, total })}>
      <div className="flex flex-1 gap-1">
        {Array.from({ length: total }, (_, i) => (
          <span key={i} className={`h-1.5 flex-1 rounded-full ${i < current ? "bg-primary" : "bg-line-strong"}`} />
        ))}
      </div>
      <span className="text-[12px] font-bold text-ink-soft">{t("setup.stepOf", { n: current, total })}</span>
    </div>
  );
}

export function LanguageSwitch() {
  const { locale, setLocale, t } = useI18n();
  return (
    <div role="group" aria-label={t("app.language")} className="inline-flex rounded-full border border-chrome-soft bg-chrome-soft p-1">
      {(["fr", "en"] as const).map((l) => (
        <button
          key={l}
          type="button"
          aria-pressed={locale === l}
          onClick={() => setLocale(l)}
          className={`min-h-9 min-w-11 rounded-full px-3 text-[13px] font-bold uppercase ${
            locale === l ? "bg-mint-bright text-chrome" : "text-chrome-text"
          }`}
        >
          {l}
        </button>
      ))}
    </div>
  );
}

/** Tappable list row (settings, history). */
export function Row({ onClick, icon, title, subtitle, trailing }: { onClick?: () => void; icon?: ReactNode; title: ReactNode; subtitle?: ReactNode; trailing?: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="focus-ring flex min-h-16 w-full items-center gap-3 rounded-card border border-line bg-white px-4 py-3 text-start shadow-card active:bg-surface-soft"
    >
      {icon && <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-primary-surface text-primary">{icon}</span>}
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[15.5px] font-bold text-ink">{title}</span>
        {subtitle && <span className="mt-0.5 block text-[13px] leading-snug text-ink-soft">{subtitle}</span>}
      </span>
      {trailing}
    </button>
  );
}
