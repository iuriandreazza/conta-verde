import { describe, expect, it } from "vitest";
import {
  DUE_BUCKETS,
  MONTHLY_FLOW,
  NEXT_30_DAYS,
  OVERDUE_RECEIVABLE,
  PERIODS,
  PERIOD_SUMMARY,
} from "./sampleData";

// Cents as BigInt: the check must not itself round the way a Number would.
const cents = (value: string) => BigInt(value.replace(".", ""));
const sum = (values: string[]) => values.reduce((total, value) => total + cents(value), 0n);

describe("dashboard sample data", () => {
  it.each(PERIODS)("totals the last $months months the way the summary says", ({ months }) => {
    const slice = MONTHLY_FLOW.slice(-months);
    const summary = PERIOD_SUMMARY[months];
    expect(sum(slice.map((month) => month.inflow))).toBe(cents(summary.inflow));
    expect(sum(slice.map((month) => month.outflow))).toBe(cents(summary.outflow));
    expect(cents(summary.inflow) - cents(summary.outflow)).toBe(cents(summary.result));
  });

  it("adds up the amounts due within 30 days", () => {
    const soon = DUE_BUCKETS.slice(1, 3);
    expect(sum(soon.map((bucket) => bucket.receivable))).toBe(cents(NEXT_30_DAYS.receivable));
    expect(sum(soon.map((bucket) => bucket.payable))).toBe(cents(NEXT_30_DAYS.payable));
  });

  it("takes the overdue receivable from the overdue bucket", () => {
    expect(DUE_BUCKETS[0].overdue).toBe(true);
    expect(DUE_BUCKETS[0].receivable).toBe(OVERDUE_RECEIVABLE.amount);
  });
});
