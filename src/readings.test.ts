import { describe, expect, it } from "vitest";
import { budgetCentsFor, readingsFor } from "./readings";
import type { ProviderView, UsageSnapshot } from "./types";

function provider(options: Record<string, string> = {}): ProviderView {
  return {
    id: "anthropic",
    name: "Anthropic",
    enabled: true,
    authMode: "auto",
    usesOauth: false,
    oauthStatus: null,
    needsKey: true,
    manualEntry: false,
    hasKey: true,
    fingerprint: null,
    caps: { cost: true, tokens: true, balance: false, series: true },
    requiredOptions: [],
    options,
    pollSeconds: 300,
    linkUrl: null,
    linkLabel: null,
  };
}

function snapshot(patch: Partial<UsageSnapshot> = {}): UsageSnapshot {
  return {
    provider: "anthropic",
    status: "ok",
    costCents: null,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    balanceCents: null,
    limits: { fiveHour: null, week: null },
    series: [],
    fetchedAt: new Date().toISOString(),
    sourceLatencySecs: 0,
    message: null,
    ...patch,
  };
}

const window_ = (remaining: number) => ({
  usedPercent: 100 - remaining,
  remainingPercent: remaining,
  resetsAt: new Date(Date.now() + 3_600_000).toISOString(),
  windowMinutes: 300,
});

describe("readingsFor", () => {
  /** An empty list is a legitimate answer. The caller shows the status instead
   *  of being handed a meter with nothing behind it. */
  it("returns nothing when there is no snapshot at all", () => {
    expect(readingsFor(provider(), undefined)).toEqual([]);
    expect(readingsFor(provider(), snapshot())).toEqual([]);
  });

  /** Every meter in the app drains in the same direction, so a window that is
   *  24% used must read as 76% left. */
  it("reports rate-limit windows as what is left", () => {
    const out = readingsFor(
      provider(),
      snapshot({ limits: { fiveHour: window_(76), week: window_(59) } }),
    );
    expect(out.map((r) => r.key)).toEqual(["fiveHour", "week"]);
    expect(out[0].value).toBe("76%");
    expect(out[1].value).toBe("59%");
  });

  /** Which ramp a meter wears says what kind of limit it is, not how full it
   *  is — so these two never swap. */
  it("keeps the short window warm and the long window cool", () => {
    const out = readingsFor(
      provider(),
      snapshot({ limits: { fiveHour: window_(50), week: window_(50) } }),
    );
    expect(out[0].ramp).toBe("warm");
    expect(out[1].ramp).toBe("cool");
  });

  /** Balance alone has no ceiling to be a fraction of. A meter would need a
   *  denominator, and inventing one is worse than showing no meter. */
  it("gives a bare balance a reading but no percentage", () => {
    const [reading] = readingsFor(provider(), snapshot({ balanceCents: 2_582 }));
    expect(reading.key).toBe("balance");
    expect(reading.value).toBe("$25.82");
    expect(reading.percent).toBeNull();
  });

  it("measures a balance against what was purchased once spend is known", () => {
    const [reading] = readingsFor(
      provider(),
      snapshot({ balanceCents: 7_500, costCents: 2_500 }),
    );
    // 75 of the 100 bought is still there.
    expect(reading.percent).toBeCloseTo(75);
  });

  it("shows spend against a budget as budget remaining", () => {
    const [reading] = readingsFor(
      provider({ budget_usd: "50" }),
      snapshot({ costCents: 2_000 }),
    );
    expect(reading.key).toBe("cost");
    expect(reading.longLabel).toBe("Budget remaining");
    expect(reading.percent).toBeCloseTo(60);
  });

  /** Over budget is 0% left, never a negative meter. */
  it("floors an overspent budget at zero rather than going negative", () => {
    const [reading] = readingsFor(
      provider({ budget_usd: "10" }),
      snapshot({ costCents: 2_500 }),
    );
    expect(reading.percent).toBe(0);
  });

  it("shows spend with no meter when no budget is set", () => {
    const [reading] = readingsFor(provider(), snapshot({ costCents: 2_500 }));
    expect(reading.longLabel).toBe("Spend");
    expect(reading.percent).toBeNull();
  });

  /** A subscription's own windows are the real limits; a budget is a number
   *  the user chose to watch. Showing both would be two answers to one question. */
  it("prefers rate-limit windows over a budget when both exist", () => {
    const out = readingsFor(
      provider({ budget_usd: "50" }),
      snapshot({ limits: { fiveHour: window_(80), week: null }, costCents: 2_000 }),
    );
    expect(out.map((r) => r.key)).toEqual(["fiveHour"]);
  });
});

describe("budgetCentsFor", () => {
  it("reads dollars from the option field as cents", () => {
    expect(budgetCentsFor(provider({ budget_usd: "25.5" }))).toBe(2_550);
  });

  it("treats an absent or unparseable budget as no budget", () => {
    expect(budgetCentsFor(provider())).toBeNull();
    expect(budgetCentsFor(provider({ budget_usd: "" }))).toBeNull();
    expect(budgetCentsFor(provider({ budget_usd: "abc" }))).toBeNull();
  });
});
