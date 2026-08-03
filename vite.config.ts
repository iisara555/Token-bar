import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

// Tauri expects a fixed port and never obscures Rust errors on the Vite side.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5183,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    // Both webviews this ships into: WebView2 (Chromium) on Windows and
    // WKWebView (Safari) on macOS. Targeting Chromium alone let esbuild emit
    // syntax Safari 15 cannot parse, which fails as a blank window rather than
    // as a build error — the macOS floor is set by Tauri's own 10.15 minimum,
    // so it has to be named explicitly.
    target: ["chrome110", "safari15"],
    sourcemap: process.env.TAURI_ENV_DEBUG === "true",
    minify: process.env.TAURI_ENV_DEBUG === "true" ? false : "esbuild",
    rollupOptions: {
      input: {
        // One HTML entry per window surface.
        bar: resolve(__dirname, "index.html"),
        popover: resolve(__dirname, "popover.html"),
        settings: resolve(__dirname, "settings.html"),
      },
    },
  },
});
