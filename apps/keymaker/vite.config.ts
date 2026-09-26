import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// `tauri android dev` serves the UI to the phone over the LAN and sets
// TAURI_DEV_HOST; desktop dev uses localhost. Port 1421 so it can run next to
// the desktop app's 1420.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  test: {
    environment: "node",
    globals: false,
  },
  server: {
    port: 1421,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1431 } : undefined,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "chrome110",
    outDir: "dist",
    // No source maps in the APK.
    sourcemap: false,
  },
});
