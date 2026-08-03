/**
 * Set the surface attributes before React mounts.
 *
 * The app is black — there is no light variant to resolve and nothing to read
 * from the OS. This still has to run before first paint: without `data-glass`
 * the surface renders with no fill at all, which on a transparent window is a
 * flash of whatever is behind it.
 *
 * `data-os` lands in the same pass because the compositor workarounds and the
 * scrollbar treatment keyed off it are structural, and applying them one frame
 * late is a visible reflow on every window open.
 */
export function bootTheme() {
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
