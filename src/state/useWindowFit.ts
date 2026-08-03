import { useEffect } from "react";
import { api } from "../api";

/**
 * Give a fixed-size window the viewport its layout was drawn for.
 *
 * `tauri.conf.json` states these sizes in logical pixels, and Tauri converts
 * them with `scale_factor()` — which on Windows follows the *display* scaling
 * only. Raising Windows' text size (Settings → Accessibility → Text size) does
 * not move it, but WebView2 honours that setting and lays the page out at a
 * matching `devicePixelRatio`. At 110% text the two disagree by a tenth: a
 * window created 760 physical pixels wide hands its page 691 CSS pixels, and a
 * layout built for 760 spends the difference clipping its own right edge.
 *
 * The webview is the only side that knows the ratio it rendered at, so it does
 * the conversion and hands back physical pixels. Runs once per mount and never
 * again, so a window the user has resized is left alone.
 *
 * `css` must stay in step with the `width`/`height` for this window's label in
 * `tauri.conf.json`.
 */
export function useWindowFit(label: string, css: { width: number; height: number }) {
  const { width, height } = css;

  useEffect(() => {
    const dpr = window.devicePixelRatio || 1;
    // Already correct — and on a machine where the two scales agree this would
    // be a no-op resize that costs a repaint on every window open.
    if (Math.abs(dpr - 1) < 0.001) return;
    void api.windowFit(label, Math.ceil(width * dpr), Math.ceil(height * dpr));
  }, [label, width, height]);
}
