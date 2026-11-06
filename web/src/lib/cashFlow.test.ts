import { describe, expect, it } from "vitest";
import type { CashFlowPeriod } from "../api";
import { defaultSpan, periodLabel, totalsOf } from "./cashFlow";

const period = (overrides: Partial<CashFlowPeriod> = {}): CashFlowPeriod => ({
  start: "2026-12-01",
  end: "2026-12-31",
  received: "0",
  paid: "0",
  transfers_in: "0",
  transfers_out: "0",
  net: "0",
  new_opening_balances: "0",
  closing_balance: "0",
  to_receive: null,
  to_pay: null,
  projected_balance: null,
  ...overrides,
});

describe("defaultSpan", () => {
  it("covers this month and the two after it", () => {
    expect(defaultSpan("2026-11-05")).toEqual({ from: "2026-11-01", to: "2027-01-31" });
    expect(defaultSpan("2026-12-31")).toEqual({ from: "2026-12-01", to: "2027-02-28" });
    expect(defaultSpan("2027-12-10")).toEqual({ from: "2027-12-01", to: "2028-02-29" });
    expect(defaultSpan("2026-01-01")).toEqual({ from: "2026-01-01", to: "2026-03-31" });
  });
});

describe("periodLabel", () => {
  it("names a month by its name and a day by its date", () => {
    expect(periodLabel(period(), "month")).toBe("dez/2026");
    expect(periodLabel(period({ start: "2026-12-05" }), "day")).toBe("05/12");
  });
});

describe("totalsOf", () => {
  it("adds up to the cent without turning an amount into a float", () => {
    const periods = [period({ received: "0.1", paid: "1000000000000.01" }), period({ received: "0.2", paid: "0.02" })];
    expect(totalsOf(periods)).toEqual({ received: "0.3", paid: "1000000000000.03" });
    expect(totalsOf([])).toEqual({ received: "0", paid: "0" });
  });
});
