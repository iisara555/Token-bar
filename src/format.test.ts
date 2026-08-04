import { describe, expect, it } from "vitest";
import { hotkeyLabel, money, relativeTime, resetIn, resetInShort, tokens } from "./format";

describe("money", () => {
  /** The case the extra decimals exist for: a working integration on a quiet
   *  day spends fractions of a cent, and "$0.00" makes it look broken. */
  it("keeps sub-cent amounts visible instead of rounding them to zero", () => {
    expect(money(0)).toBe("$0.00");
    expect(money(0.4)).toBe("$0.0040");
    expect(money(1)).toBe("$0.01");
  });

  it("drops to whole dollars once the cents stop mattering", () => {
    expect(money(1_234)).toBe("$12.34");
    expect(money(99_999)).toBe("$999.99");
    expect(money(123_456)).toBe("$1,235");
  });

  /** Not zero. A provider that reported nothing and a provider that spent
   *  nothing are different facts and must not print the same. */
  it("renders an absent figure as a dash", () => {
    expect(money(null)).toBe("—");
    expect(money(undefined)).toBe("—");
  });
});

describe("tokens", () => {
  it("compacts at each magnitude", () => {
    expect(tokens(999)).toBe("999");
    expect(tokens(1_200)).toBe("1.2K");
    expect(tokens(15_000)).toBe("15K");
    expect(tokens(1_200_000)).toBe("1.2M");
    expect(tokens(15_000_000)).toBe("15M");
    expect(tokens(1_200_000_000)).toBe("1.2B");
  });
});

describe("relativeTime", () => {
  it("reads as a rough age, not a timestamp", () => {
    const ago = (ms: number) => new Date(Date.now() - ms).toISOString();
    expect(relativeTime(ago(10_000))).toBe("just now");
    expect(relativeTime(ago(5 * 60_000))).toBe("5m ago");
    expect(relativeTime(ago(3 * 3_600_000))).toBe("3h ago");
    expect(relativeTime(ago(2 * 86_400_000))).toBe("2d ago");
  });

  it("survives an unparseable timestamp", () => {
    expect(relativeTime("not a date")).toBe("never");
  });
});

describe("reset countdowns", () => {
  const inMs = (ms: number) => new Date(Date.now() + ms).toISOString();

  it("counts down in the unit that is still meaningful", () => {
    expect(resetInShort(inMs(30 * 60_000))).toBe("30m");
    expect(resetInShort(inMs(3 * 3_600_000))).toBe("3h");
    expect(resetInShort(inMs(4 * 86_400_000))).toBe("4d");
  });

  /** A window whose reset has passed is resetting, not overdue by a negative
   *  amount — the bar should never print "-3m". */
  it("says a passed reset is happening now", () => {
    expect(resetIn(inMs(-60_000))).toBe("resetting now");
    expect(resetInShort(inMs(-60_000))).toBe("now");
  });

  it("admits when there is no reset time to show", () => {
    expect(resetIn(null)).toBe("reset time unavailable");
    expect(resetInShort(undefined)).toBe("—");
  });
});

describe("hotkeyLabel", () => {
  it("writes the shortcut the way Windows writes it", () => {
    expect(hotkeyLabel("CmdOrControl+Alt+U", false)).toBe("Ctrl+Alt+U");
  });

  /** Apple's canonical modifier order is ⌃⌥⇧⌘ regardless of how the
   *  accelerator was typed, because a Mac user reads it as one glyph cluster. */
  it("writes the same shortcut the way macOS writes it", () => {
    expect(hotkeyLabel("CmdOrControl+Alt+U", true)).toBe("⌥⌘U");
    expect(hotkeyLabel("Shift+Ctrl+Alt+Cmd+K", true)).toBe("⌃⌥⇧⌘K");
  });
});
