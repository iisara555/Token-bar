import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { enabledProviders, useUsage } from "../state/useUsage";
import { ProviderChip } from "../components/ProviderChip";
import { DetailPanel } from "../components/DetailPanel";
import { Wordmark } from "../components/Wordmark";
import { RefreshIcon } from "../components/Icons";
import type { ProviderId } from "../types";

export function FloatingBar() {
  const { view, snapshots, ready, error, init, refresh } = useUsage();
  const [expanded, setExpanded] = useState<ProviderId | null>(null);
  const [spinning, setSpinning] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void init();
  }, [init]);

  useAutoSize(rootRef, [view, snapshots, expanded, ready]);

  // The bar is dragged by the window manager, so React never sees a drag end.
  // A global mouseup is the only reliable point to ask Rust to snap and save.
  //
  // Only a press that landed on the drag region can have started a drag, and
  // that distinction is load-bearing: unconditionally, every click of Refresh or
  // Settings rewrote config.json and re-ran the edge snap, which could jump the
  // window out from under the pointer when the bar was sitting near an edge.
  useEffect(() => {
    let fromDragRegion = false;

    const onDown = (e: MouseEvent) => {
      fromDragRegion =
        e.target instanceof Element &&
        e.target.closest("[data-tauri-drag-region]") !== null;
    };
    const onUp = () => {
      if (!fromDragRegion) return;
      fromDragRegion = false;
      void api.barDropped();
    };

    window.addEventListener("mousedown", onDown);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  const onRefresh = useCallback(async () => {
    setSpinning(true);
    await refresh();
    setTimeout(() => setSpinning(false), 900);
  }, [refresh]);

  if (!ready) {
    return <div className="bar-root" ref={rootRef} />;
  }

  if (error) {
    return (
      <div className="bar-root" ref={rootRef}>
        <div className="glass glass--pill bar" data-tauri-drag-region>
          <Wordmark />
          <span className="bar-error">{error}</span>
        </div>
      </div>
    );
  }

  const providers = enabledProviders(view);
  const active = providers.find((p) => p.id === expanded);

  return (
    <div className="bar-root" ref={rootRef}>
      <div
        className="glass glass--pill bar"
        data-compact={view?.compact}
        data-tauri-drag-region
      >
        <Wordmark />

        {providers.length === 0 ? (
          <span className="bar-error">No providers enabled</span>
        ) : (
          providers.map((p) => (
            <div className="bar-cell" key={p.id}>
              <span className="bar-divider" aria-hidden />
              <ProviderChip
                provider={p}
                snapshot={snapshots[p.id]}
                expanded={expanded === p.id}
                onToggle={() => setExpanded(expanded === p.id ? null : p.id)}
              />
            </div>
          ))
        )}

        <button
          type="button"
          className="icon-btn bar-refresh"
          data-spinning={spinning}
          onClick={onRefresh}
          title="Refresh now"
          onContextMenu={(e) => {
            // The bar has no room for a settings control at this density, and
            // the tray menu is the discoverable route. Right-click here is the
            // shortcut for anyone who never goes to the tray.
            e.preventDefault();
            void api.openSettings();
          }}
        >
          <RefreshIcon />
          <span className="sr-only">Refresh all providers</span>
        </button>
      </div>

      {active && (
        <DetailPanel
          provider={active}
          snapshot={snapshots[active.id]}
          windowDays={view?.windowDays ?? 30}
        />
      )}
    </div>
  );
}

/**
 * Keep the window exactly as big as its contents.
 *
 * The bar has no fixed width — it grows with the number of enabled providers
 * and grows again when a card opens. Sizing the OS window to match is what
 * stops the transparent window from swallowing clicks in the empty space
 * around the pill.
 */
function useAutoSize(
  ref: React.RefObject<HTMLDivElement | null>,
  deps: unknown[],
) {
  const lastSent = useRef<{ w: number; h: number }>({ w: 0, h: 0 });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;

    const send = () => {
      const children = Array.from(el.children).filter(
        (child): child is HTMLElement => child instanceof HTMLElement,
      );
      if (children.length === 0) return;

      // `offsetWidth`, not `getBoundingClientRect()`: the card animates in from
      // `scale(0.985)`, and a rect is the *painted* box, so opening a card
      // measured it about 1.5% narrow — and nothing corrected it, because a
      // ResizeObserver watches the border box, which a transform never changes.
      // These are layout metrics and ignore transforms entirely.
      const cssW = Math.max(el.offsetWidth, ...children.map((c) => c.offsetWidth + 24));
      const cssH = Math.max(el.scrollHeight, el.offsetHeight);

      // Convert here, not in Rust. These are CSS pixels; the OS window is sized
      // in physical ones, and `devicePixelRatio` is the only honest exchange
      // rate — Win32 reports 96 DPI regardless of Windows' text-size setting,
      // which WebView2 *does* apply, so the native side's `scale_factor()` reads
      // 1.0 while the page is really laid out at 1.1. See `bar_set_size`.
      const dpr = window.devicePixelRatio || 1;
      const w = Math.ceil(cssW * dpr);
      const h = Math.ceil(cssH * dpr);

      // Resizing the window re-fires the observer; without this guard the two
      // would trade one-pixel corrections forever.
      if (Math.abs(w - lastSent.current.w) < 2 && Math.abs(h - lastSent.current.h) < 2) {
        return;
      }
      lastSent.current = { w, h };
      void api.barSetSize(w, h);
    };

    send();
    const ro = new ResizeObserver(send);
    ro.observe(el);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}
