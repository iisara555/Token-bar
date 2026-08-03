/**
 * Set a theme before React mounts.
 *
 * Without this the first paint happens with no `data-theme` at all, which on a
 * transparent window shows as a white flash over whatever is behind it. The
 * stored preference arrives a tick later and corrects this if it differs.
 */
export function bootTheme() {
  const dark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  document.documentElement.dataset.glass ??= "css";
}
