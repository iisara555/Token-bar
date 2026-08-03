/**
 * The wordmark that opens the bar.
 *
 * Set in the same dot-matrix face as every reading beside it, which is the
 * point: the name is the first instrument on the panel rather than a logo
 * sitting next to one. The product is still called Token Bar everywhere a
 * window title or a tray tooltip needs a name — this is a logotype, not a
 * rename.
 */
export function Wordmark() {
  return (
    <span className="wordmark num" data-tauri-drag-region>
      Quoken
    </span>
  );
}
