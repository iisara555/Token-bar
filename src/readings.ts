import type { Ramp } from "./components/Meter";
import { money, resetAt, resetIn, resetInShort } from "./format";
import { budgetCentsFor } from "./state/useUsage";
import type { ProviderView, UsageSnapshot } from "./types";

/**
 * One row of the instrument panel.
 *
 * The bar and the detail card show the same readings at two sizes, so they are
 * derived once here rather than assembled twice. Anything that would have to
 * agree between the two — which ramp, what the percentage means, how the reset
 * is worded — can then only be got wrong in one place.
 */
export interface Reading {
  key: string;
  /** Toolbar tag: "5H", "W", "$". */
  shortLabel: string;
  /** Status panel column: "5 Hour", "Weekly", "Total available". */
  panelLabel: string;
  /** Card heading: "5 hour limit". */
  longLabel: string;
  /** 0–100, or null when the provider reports no ceiling to measure against. */
  percent: number | null;
  value: string;
  /** Under the toolbar track: "Reset 3h". */
  shortCaption?: string;
  /** Under the card track: "Reset in 3h : Aug 3, 12:50 PM". */
  longCaption?: string;
  ramp: Ramp;
  description: string;
}

/**
 * What this provider has to say, in the order it should be read.
 *
 * Providers differ in what they will tell you: subscription plans report rate
 * limit windows, pay-as-you-go accounts report a balance, and some report only
 * a cost. Rather than forcing all of them into one shape and inventing the
 * missing halves, each gets the readings it can actually support — an empty
 * list is a legitimate answer and the caller shows the status instead.
 */
export function readingsFor(
  provider: ProviderView,
  snapshot: UsageSnapshot | undefined,
): Reading[] {
  if (!snapshot) return [];

  const out: Reading[] = [];
  const { fiveHour, week } = snapshot.limits ?? {};

  // Rate limit windows. These report what is *left*, which is what the meter
  // shows: a full bar is a full tank, and it drains as the window is consumed.
  if (fiveHour) {
    const remaining = Math.round(fiveHour.remainingPercent);
    out.push({
      key: "fiveHour",
      shortLabel: "5H",
      panelLabel: "5 Hour",
      longLabel: "5 hour limit",
      percent: remaining,
      value: `${remaining}%`,
      shortCaption: `Reset ${resetInShort(fiveHour.resetsAt)}`,
      longCaption: resetLine(fiveHour.resetsAt),
      ramp: "warm",
      description: `${provider.name} 5-hour limit: ${remaining}% left, ${resetIn(fiveHour.resetsAt)}`,
    });
  }

  if (week) {
    const remaining = Math.round(week.remainingPercent);
    out.push({
      key: "week",
      shortLabel: "W",
      panelLabel: "Weekly",
      longLabel: "Weekly limit",
      percent: remaining,
      value: `${remaining}%`,
      shortCaption: `Reset ${resetInShort(week.resetsAt)}`,
      longCaption: resetLine(week.resetsAt),
      ramp: "cool",
      description: `${provider.name} weekly limit: ${remaining}% left, ${resetIn(week.resetsAt)}`,
    });
  }

  if (out.length > 0) return out;

  // Pay-as-you-go. With both halves known, the honest denominator is what was
  // bought — balance plus what has already been spent out of it. Balance alone
  // has no ceiling to be a fraction of, so it gets a reading with no meter
  // rather than a meter with a made-up scale.
  const { balanceCents, costCents } = snapshot;
  if (balanceCents !== null && balanceCents !== undefined) {
    const spent = costCents ?? null;
    const purchased = spent !== null ? balanceCents + spent : null;
    const percent =
      purchased !== null && purchased > 0 ? (balanceCents / purchased) * 100 : null;

    out.push({
      key: "balance",
      shortLabel: "$",
      panelLabel: "Total available",
      longLabel: "Total available",
      percent,
      value: money(balanceCents),
      shortCaption: spent !== null ? `Spent ${money(spent)}` : undefined,
      longCaption: "Pay-as-you-go balance",
      ramp: "warm",
      description:
        percent === null
          ? `${provider.name} balance: ${money(balanceCents)}`
          : `${provider.name} balance: ${money(balanceCents)}, ${Math.round(percent)}% of credits purchased still available`,
    });
    return out;
  }

  // Cost against a budget the user set here. Shown as remaining so it drains
  // the same direction as every other meter — a bar that filled up as things
  // got worse would be the only one in the app running backwards.
  const budget = budgetCentsFor(provider);
  if (costCents !== null && costCents !== undefined) {
    const percent =
      budget !== null && budget > 0
        ? Math.max(0, (1 - costCents / budget) * 100)
        : null;

    out.push({
      key: "cost",
      shortLabel: "$",
      panelLabel: budget === null ? "Spend" : "Budget left",
      longLabel: budget === null ? "Spend" : "Budget remaining",
      percent,
      value: money(costCents),
      shortCaption: budget !== null ? `of ${money(budget)}` : undefined,
      longCaption:
        budget === null
          ? "No budget set"
          : `${money(costCents)} of ${money(budget)} used`,
      ramp: "warm",
      description:
        percent === null
          ? `${provider.name} spend: ${money(costCents)}`
          : `${provider.name}: ${money(costCents)} of ${money(budget)} budget, ${Math.round(percent)}% left`,
    });
  }

  return out;
}

/**
 * The single meter at the top of the status panel: how much headroom is left at
 * the tightest constraint anywhere.
 *
 * Tightest, not average. Averaging is what turns "the weekly limit is 2% from
 * gone" into a comfortable-looking 70%, and a summary that can only ever look
 * calmer than reality is worse than no summary.
 *
 * Note this is deliberately *not* a bar for the spend figure it sits under.
 * Most accounts have no budget set, so a spend meter would have no denominator
 * and would be inventing its own scale — and where a budget does exist it is
 * per-provider, so there is no honest way to add them into one bar. Headroom is
 * the thing that is always defined and always comparable.
 */
export function tightestHeadroom(
  entries: { provider: ProviderView; snapshot: UsageSnapshot | undefined }[],
): { percent: number; label: string; ramp: Ramp } | null {
  let worst: { percent: number; label: string; ramp: Ramp } | null = null;

  for (const { provider, snapshot } of entries) {
    for (const r of readingsFor(provider, snapshot)) {
      if (r.percent === null) continue;
      if (worst === null || r.percent < worst.percent) {
        worst = {
          percent: r.percent,
          label: `Tightest: ${provider.name} ${r.panelLabel.toLowerCase()}, ${Math.round(r.percent)}% left`,
          // The ramp comes from the reading itself rather than being fixed
          // warm. This bar *is* that reading shown large, so letting it wear a
          // different ramp than the row it came from would be the one place in
          // the app where the ramp stops naming the kind of limit.
          ramp: r.ramp,
        };
      }
    }
  }

  return worst;
}

/** "Reset in 3h : Aug 3, 12:50 PM" — the countdown and the wall clock together. */
function resetLine(iso: string | null | undefined): string {
  if (!iso) return "Reset time unavailable";
  // `resetIn` phrases it as a sentence fragment; the card wants a title.
  const relative = resetIn(iso).replace(/^resets in /, "Reset in ").replace(/^resetting now$/, "Resetting now");
  if (!iso || relative === "reset time unavailable") return "Reset time unavailable";
  return `${relative} : ${resetAt(iso)}`;
}

/** Whether this provider is running on a signed-in session or a pasted key. */
export function authKind(provider: ProviderView): "oauth" | "api" {
  if (provider.authMode === "api_key") return "api";
  if (provider.authMode === "oauth") return "oauth";
  // `auto` prefers an existing official-client login where one is possible.
  return provider.usesOauth && provider.oauthStatus === "connected" ? "oauth" : "api";
}
