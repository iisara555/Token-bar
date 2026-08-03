import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../api";
import { enabledProviders, grandTotal, guardLoad, useUsage } from "../state/useUsage";
import { money } from "../format";
import { ProviderChip } from "../components/ProviderChip";
import { DetailPanel } from "../components/DetailPanel";
import { StatusBar } from "../components/StatusBar";
import { RefreshIcon, SettingsIcon } from "../components/Icons";
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
        <div className="glass glass--pill bar-shell" data-compact={view?.compact}>
          <div className="bar" data-tauri-drag-region>
            <span style={{ fontSize: "var(--text-sm)", padding: "0 8px" }}>{error}</span>
          </div>
        </div>
      </div>
    );
  }

  const providers = enabledProviders(view);
  const total = grandTotal(view, snapshots);
  const active = providers.find((p) => p.id === expanded);
  const load = guardLoad(view, snapshots);
  const hasQuotaProviders = providers.some((p) => {
    const limits = snapshots[p.id]?.limits;
    return Boolean(limits?.fiveHour || limits?.week);
  });

  return (
    <div className="bar-root" ref={rootRef}>
      <div className="glass glass--pill bar-shell" data-compact={view?.compact}>
        <div className="bar" data-tauri-drag-region>
          <span className="bar-grip" data-tauri-drag-region aria-hidden />

          {providers.length === 0 ? (
            <span className="dim" style={{ fontSize: "var(--text-sm)", padding: "0 6px" }}>
              No providers enabled
            </span>
          ) : (
            providers.map((p) => (
              <ProviderChip
                key={p.id}
                provider={p}
                snapshot={snapshots[p.id]}
                expanded={expanded === p.id}
                onToggle={() => setExpanded(expanded === p.id ? null : p.id)}
              />
            ))
          )}

          <span className="divider" aria-hidden />

          <span className="total">
            <span className="total-value num">
              {total.reporting > 0
                ? money(total.cents)
                : hasQuotaProviders
                  ? "LIVE"
                  : money(total.cents)}
            </span>
            <span className="total-label">
              {/* Never present a partial sum as the whole picture. */}
              {total.reporting === 0 && hasQuotaProviders
                ? "limits remaining"
                : total.reporting === total.total
                ? `${view?.windowDays ?? 30}d total`
                : `${total.reporting}/${total.total} reporting`}
            </span>
          </span>

          <span className="bar-actions">
            <button
              type="button"
              className="icon-btn"
              data-spinning={spinning}
              onClick={onRefresh}
              title="Refresh now"
            >
              <RefreshIcon />
              <span className="sr-only">Refresh</span>
            </button>
            <button
              type="button"
              className="icon-btn"
              onClick={() => void api.openSettings()}
              title="Settings"
            >
              <SettingsIcon />
              <span className="sr-only">Settings</span>
            </button>
          </span>
        </div>

        <StatusBar load={load} warnAt={view?.warnAt ?? 0.8} />
      </div>

      {active && (
        <DetailPanel
          provider={active}
          snapshot={snapshots[active.id]}
          warnAt={view?.warnAt ?? 0.8}
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
 * and doubles in height when a panel opens. Sizing the OS window to match is
 * what stops the transparent window from swallowing clicks in the empty space
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
      const children = Array.from(el.children);
      if (children.length === 0) return;
      // Root padding is shadow room; include it so nothing is clipped.
      // The detail panel can be wider than a short bar, so size for whichever
      // visible surface currently needs the most room.
      const w = Math.ceil(
        Math.max(...children.map((child) => child.getBoundingClientRect().width)) + 16,
      );
      const h = Math.ceil(el.scrollHeight);
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
