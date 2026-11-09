import { describe, expect, it } from "vitest";
import type { DreLines } from "../api";
import { DRE_ROWS, earning, monthColumnLabel, spending } from "./dre";

const lines = (values: Partial<DreLines>): DreLines => ({
  gross_revenue: "0",
  deductions: "0",
  costs: "0",
  operating_expenses: "0",
  financial_result: "0",
  investments: "0",
  outside_result: "0",
  net_revenue: "0",
  gross_profit: "0",
  operating_result: "0",
  net_result: "0",
  ...values,
});

describe("the income statement's lines", () => {
  it("are read in order, the result before what is below the line", () => {
    expect(DRE_ROWS.map((row) => row.key)).toEqual([
      "gross_revenue",
      "deductions",
      "net_revenue",
      "costs",
      "gross_profit",
      "operating_expenses",
      "operating_result",
      "financial_result",
      "net_result",
      "investments",
      "outside_result",
    ]);
    expect(DRE_ROWS.filter((row) => row.belowTheLine).map((row) => row.key)).toEqual(["investments", "outside_result"]);
  });

  it("names a month the way a column wants it", () => {
    expect(monthColumnLabel("2026-10")).toBe("out/2026");
    expect(monthColumnLabel("2027-01")).toBe("jan/2027");
  });

  it("draws spending and earning as positive amounts, exactly", () => {
    expect(spending(lines({ costs: "-300.1", operating_expenses: "-200.2" }))).toBe("500.3");
    expect(spending(lines({ costs: "-300", operating_expenses: "400" }))).toBe("0");
    expect(earning(lines({ net_revenue: "1370.55" }))).toBe("1370.55");
    expect(earning(lines({ net_revenue: "-5" }))).toBe("0");
  });
});
