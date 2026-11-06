import { describe, expect, it } from "vitest";
import { CUSTOM, PRESETS, monthsSpan, presetOf, shiftSpan } from "./period";

describe("monthsSpan", () => {
  it("runs from the first day to the last of whole months, across a year and in a leap year", () => {
    expect(monthsSpan(2026, 11, 3)).toEqual({ from: "2026-11-01", to: "2027-01-31" });
    expect(monthsSpan(2028, 2, 1)).toEqual({ from: "2028-02-01", to: "2028-02-29" });
    expect(monthsSpan(2026, 0, 1)).toEqual({ from: "2025-12-01", to: "2025-12-31" });
  });
});

describe("presets", () => {
  const today = "2026-11-06";
  const spanOf = (key: string) => PRESETS.find((preset) => preset.key === key)!.span(today);

  it("are the months, the quarter and the year around today", () => {
    expect(spanOf("this-month")).toEqual({ from: "2026-11-01", to: "2026-11-30" });
    expect(spanOf("last-month")).toEqual({ from: "2026-10-01", to: "2026-10-31" });
    expect(spanOf("next-month")).toEqual({ from: "2026-12-01", to: "2026-12-31" });
    expect(spanOf("three-months")).toEqual({ from: "2026-11-01", to: "2027-01-31" });
    expect(spanOf("quarter")).toEqual({ from: "2026-10-01", to: "2026-12-31" });
    expect(spanOf("year")).toEqual({ from: "2026-01-01", to: "2026-12-31" });
  });

  it("are recognised from the span, and any other span is custom", () => {
    expect(presetOf({ from: "2026-11-01", to: "2026-11-30" }, today)).toBe("this-month");
    expect(presetOf({ from: "2026-11-02", to: "2026-11-30" }, today)).toBe(CUSTOM);
  });
});

describe("shiftSpan", () => {
  it("moves a month by a month, over the end of a year", () => {
    expect(shiftSpan({ from: "2026-12-01", to: "2026-12-31" }, 1)).toEqual({ from: "2027-01-01", to: "2027-01-31" });
    expect(shiftSpan({ from: "2027-01-01", to: "2027-01-31" }, -1)).toEqual({ from: "2026-12-01", to: "2026-12-31" });
  });

  it("moves several whole months by that many months", () => {
    expect(shiftSpan({ from: "2026-11-01", to: "2027-01-31" }, 1)).toEqual({ from: "2027-02-01", to: "2027-04-30" });
    expect(shiftSpan({ from: "2026-11-01", to: "2027-01-31" }, -1)).toEqual({ from: "2026-08-01", to: "2026-10-31" });
  });

  it("moves any other span by its own number of days", () => {
    expect(shiftSpan({ from: "2026-11-10", to: "2026-11-19" }, 1)).toEqual({ from: "2026-11-20", to: "2026-11-29" });
    expect(shiftSpan({ from: "2026-11-10", to: "2026-11-19" }, -1)).toEqual({ from: "2026-10-31", to: "2026-11-09" });
  });

  it("falls back to days when the span starts on day 1 but does not end on a month's last day", () => {
    expect(shiftSpan({ from: "2026-11-01", to: "2026-11-15" }, 1)).toEqual({ from: "2026-11-16", to: "2026-11-30" });
  });

  it("moves twelve months by twelve, and a leap February by a month", () => {
    expect(shiftSpan({ from: "2026-01-01", to: "2026-12-31" }, 1)).toEqual({ from: "2027-01-01", to: "2027-12-31" });
    expect(shiftSpan({ from: "2028-02-01", to: "2028-02-29" }, 1)).toEqual({ from: "2028-03-01", to: "2028-03-31" });
    expect(shiftSpan({ from: "2028-03-01", to: "2028-03-31" }, -1)).toEqual({ from: "2028-02-01", to: "2028-02-29" });
  });
});
