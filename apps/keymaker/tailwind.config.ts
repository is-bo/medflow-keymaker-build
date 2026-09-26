import type { Config } from "tailwindcss";

/**
 * The MedFlow desktop palette (apps/desktop/tailwind.config.ts), trimmed to
 * what Key Maker uses, with phone-sized radii and touch targets.
 */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        chrome: { DEFAULT: "#0e343f", soft: "#134b5e", text: "#C8EEFA", dim: "#5f8a99", faint: "#9dc4d1" },
        primary: {
          DEFAULT: "#1A6E8A",
          hover: "#155A72",
          light: "#5BB8D4",
          pale: "#C8EEFA",
          surface: "#E0F2F8",
          wash: "#F7FBFD",
          border: "#B9E0EC",
        },
        mint: { DEFAULT: "#5BB896", bright: "#97F5D0", wash: "#ECF8F3", border: "#B7E5D3" },
        success: { DEFAULT: "#047857", surface: "#ECFDF5", border: "#A7F3D0" },
        warning: { DEFAULT: "#B45309", surface: "#FFFBEB", border: "#FDE68A" },
        danger: { DEFAULT: "#B91C1C", surface: "#FEF2F2", border: "#FCA5A5" },
        violet: { DEFAULT: "#7C3AED", surface: "#F5F3FF" },
        ink: { DEFAULT: "#11181B", body: "#344249", mid: "#4D5B61", soft: "#66767D", faint: "#B7C2C8" },
        surface: { app: "#F0F5F8", card: "#FFFFFF", chip: "#EEF4F6", input: "#F9FAFB", soft: "#F4F8FA" },
        line: { DEFAULT: "#EAF0F2", strong: "#DDE7EA", mid: "#C7D3D8" },
      },
      fontFamily: {
        sans: ["'Plus Jakarta Sans'", "system-ui", "sans-serif"],
        mono: ["ui-monospace", "'Roboto Mono'", "SFMono-Regular", "monospace"],
      },
      borderRadius: {
        DEFAULT: "8px",
        btn: "12px",
        card: "16px",
        input: "12px",
        chip: "999px",
      },
      boxShadow: {
        card: "0 1px 2px rgba(16,24,40,.04), 0 10px 26px rgba(26,110,138,.06)",
        cta: "0 6px 16px rgba(26,110,138,.25)",
        sheet: "0 -8px 30px rgba(14,52,63,.18)",
      },
      keyframes: {
        mffade: { from: { opacity: "0", transform: "translateY(6px)" }, to: { opacity: "1", transform: "none" } },
      },
      animation: { mffade: "mffade .22s ease" },
    },
  },
  plugins: [],
} satisfies Config;
