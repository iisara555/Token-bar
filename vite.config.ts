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
    target: "chrome110", // WebView2 evergreen
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
