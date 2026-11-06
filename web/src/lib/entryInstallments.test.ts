import { describe, expect, it } from "vitest";
import {
  centsToApi,
  formatDate,
  isIsoDate,
  monthlyDates,
  splitAmount,
  sumCents,
  toCents,
} from "./entryInstallments";

describe("amounts in cents", () => {
  it("reads what a Brazilian types, and refuses what is not an amount in cents", () => {
    expect(toCents("10,50")).toBe(1050n);
    expect(toCents("1.234,5")).toBe(123450n);
    expect(toCents("10.5")).toBe(1050n);
    expect(toCents("10,500")).toBe(1050n);
    expect(toCents("0,01")).toBe(1n);
    expect(toCents("9999999999999,99")).toBe(999999999999999n);
    for (const text of ["", "   ", "abc", "-1", "1,005", "1e3", "10000000000000", "1,2,3"]) {
      expect(toCents(text), text).toBeNull();
    }
  });

  it("writes cents as the plain decimal the API takes", () => {
    expect(centsToApi(10050n)).toBe("100.5");
    expect(centsToApi(10000n)).toBe("100");
    expect(centsToApi(1n)).toBe("0.01");
    expect(centsToApi(999999999999999n)).toBe("9999999999999.99");
  });

  it("adds exactly, and says nothing while an amount is not one", () => {
    expect(sumCents(["0,1", "0,2"])).toBe(30n);
    expect(sumCents(["100,50", "100,5", "0,01"])).toBe(20101n);
    expect(sumCents(["1", "x"])).toBeNull();
    expect(sumCents([])).toBe(0n);
  });

  it("splits a total with the remainder on the last share", () => {
    expect(splitAmount("100", 3)).toEqual(["33.33", "33.33", "33.34"]);
    expect(splitAmount("100", 4)).toEqual(["25", "25", "25", "25"]);
    expect(splitAmount("0,10", 3)).toEqual(["0.03", "0.03", "0.04"]);
    expect(splitAmount("2.500,00", 1)).toEqual(["2500"]);
    // The shares always add up to the total, whatever it is.
    for (const [total, count] of [["1234,56", 7], ["0,07", 7], ["999,99", 12]] as const) {
      const shares = splitAmount(total, count)!;
      expect(shares).toHaveLength(count);
      expect(sumCents(shares)).toBe(toCents(total));
    }
  });

  it("refuses a split that cannot be made", () => {
    expect(splitAmount("", 3)).toBeNull();
    expect(splitAmount("0", 3)).toBeNull();
    expect(splitAmount("abc", 3)).toBeNull();
    expect(splitAmount("100", 0)).toBeNull();
    expect(splitAmount("100", 1.5)).toBeNull();
    expect(splitAmount("0,02", 3)).toBeNull();
  });
});

describe("dates", () => {
  it("knows a real day", () => {
    expect(isIsoDate("2026-02-28")).toBe(true);
    expect(isIsoDate("2026-02-29")).toBe(false);
    expect(isIsoDate("2028-02-29")).toBe(true);
    expect(isIsoDate("2026-13-01")).toBe(false);
    expect(isIsoDate("10/12/2026")).toBe(false);
    expect(isIsoDate("")).toBe(false);
  });

  it("counts months from the first date, so the 31st does not slide for good", () => {
    expect(monthlyDates("2026-01-31", 4)).toEqual(["2026-01-31", "2026-02-28", "2026-03-31", "2026-04-30"]);
    expect(monthlyDates("2027-12-10", 3)).toEqual(["2027-12-10", "2028-01-10", "2028-02-10"]);
    expect(monthlyDates("2027-12-31", 3)).toEqual(["2027-12-31", "2028-01-31", "2028-02-29"]);
  });

  it("says nothing for what is not a date or a count", () => {
    expect(monthlyDates("2026-02-30", 3)).toBeNull();
    expect(monthlyDates("", 3)).toBeNull();
    expect(monthlyDates("2026-01-10", 0)).toBeNull();
  });

  it("shows a date the way a Brazilian reads it", () => {
    expect(formatDate("2026-12-10")).toBe("10/12/2026");
  });
});
