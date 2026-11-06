import { describe, expect, it } from "vitest";
import type { CashFlow, CashFlowPeriod } from "../api";
import { dueBuckets, granularityOf, periodLabel, spanDays } from "./figures";

const day = (start: string, to_receive: string | null, to_pay: string | null): CashFlowPeriod => ({
  start,
  end: start,
  received: "0",
  paid: "0",
  transfers_in: "0",
  transfers_out: "0",
  net: "0",
  new_opening_balances: "0",
  closing_balance: "0",
  to_receive,
  to_pay,
  projected_balance: null,
});

const flow = (periods: CashFlowPeriod[], past_due: CashFlow["past_due"]): CashFlow => ({
  from: "2026-11-06",
  to: "2027-01-05",
  granularity: "day",
  account_id: null,
  opening_balance: "0",
  past_due,
  periods,
});

describe("dueBuckets", () => {
  it("sorts what is open by how far off its day is, with what already fell due apart", () => {
    const buckets = dueBuckets(
      flow(
        [
          day("2026-11-06", "100.10", "0"),
          day("2026-11-07", "0.20", "5"),
          day("2026-11-13", "1", "1"),
          day("2026-11-14", "2", "2"),
          day("2026-12-06", "4", "4"),
          day("2026-12-07", "8", "8"),
          day("2027-01-05", "16", null),
        ],
        { to_receive: "50", to_pay: "25.5" },
      ),
      "2026-11-06",
    );
    expect(buckets.map((bucket) => [bucket.label, bucket.receivable, bucket.payable])).toEqual([
      ["Vencidas", "50", "25.5"],
      ["Hoje", "100.1", "0"],
      ["1 a 7 dias", "1.2", "6"],
      ["8 a 30 dias", "6", "6"],
      ["31 a 60 dias", "24", "8"],
    ]);
    expect(buckets[0].overdue).toBe(true);
    expect(buckets.slice(1).every((bucket) => !bucket.overdue)).toBe(true);
  });

  it("is zero everywhere when nothing is open", () => {
    const buckets = dueBuckets(flow([], null), "2026-11-06");
    expect(buckets.every((bucket) => bucket.receivable === "0" && bucket.payable === "0")).toBe(true);
  });
});

describe("periods", () => {
  it("names a day and a month the way a chart reads them", () => {
    expect(periodLabel("2026-11-06", "day")).toBe("06/11");
    expect(periodLabel("2026-11-01", "month")).toBe("nov/26");
  });

  it("counts both ends of a span and goes month by month past three of them", () => {
    expect(spanDays("2026-11-01", "2026-11-30")).toBe(30);
    expect(granularityOf("2026-11-01", "2026-11-30")).toBe("day");
    expect(granularityOf("2026-01-01", "2026-12-31")).toBe("month");
    expect(granularityOf("2026-11-01", "2027-01-31")).toBe("day");
    expect(granularityOf("2026-11-01", "2027-02-01")).toBe("month");
  });
});
