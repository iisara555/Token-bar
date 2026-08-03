/**
 * Set theme and platform before React mounts.
 *
 * Without this the first paint happens with no `data-theme` at all, which on a
 * transparent window shows as a white flash over whatever is behind it. The
 * stored preference arrives a tick later and corrects this if it differs.
 *
 * `data-os` has to land in the same pass for the same reason: the compositor
 * workarounds and the scrollbar treatment keyed off it are structural, and
 * applying them one frame late is a visible reflow on every window open.
 */
export function bootTheme() {
  const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  document.documentElement.dataset.glass ??= "css";
  document.documentElement.dataset.os ??= detectOs();
}

export type Os = "macos" | "windows" | "linux";

/**
 * Which desktop this webview is painting on.
 *
 * `navigator.userAgentData.platform` is the only one of these that is not
 * deprecated, but WKWebView does not implement it, which is precisely the case
 * this needs to get right — so the legacy fields stay as the fallback rather
 * than as a nicety. All three are read as a single haystack because WKWebView
 * reports "MacIntel" in `platform` even on Apple silicon, while the user agent
 * says "Macintosh"; either is a match and neither is guaranteed alone.
 */
export function detectOs(): Os {
  const uaData = (
    navigator as Navigator & { userAgentData?: { platform?: string } }
  ).userAgentData;
  const haystack = [
    uaData?.platform ?? "",
    navigator.platform ?? "",
    navigator.userAgent ?? "",
  ].join(" ");

  if (/mac|iphone|ipad|ipod/i.test(haystack)) return "macos";
  if (/win/i.test(haystack)) return "windows";
  return "linux";
}
