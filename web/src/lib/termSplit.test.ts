import { describe, expect, it } from "vitest";
import { addDays, splitByTerm } from "./termSplit";

const thirds = [
  { days: 30, percent: "33.33" },
  { days: 60, percent: "33.33" },
  { days: 90, percent: "33.34" },
];

describe("applying a payment term", () => {
  it("cuts each share to the cent and gives the last what is left, like the API", () => {
    // The worked case of the domain's tests.
    expect(splitByTerm(10_000n, thirds, "2026-11-30")).toEqual([
      { due: "2026-12-30", cents: 3333n },
      { due: "2027-01-29", cents: 3333n },
      { due: "2027-02-28", cents: 3334n },
    ]);
    const odd = splitByTerm(10_001n, thirds, "2026-11-30")!;
    expect(odd.map((part) => part.cents)).toEqual([3333n, 3333n, 3335n]);
    expect(odd.reduce((sum, part) => sum + part.cents, 0n)).toBe(10_001n);
  });

  it("refuses an amount too small to be split", () => {
    expect(splitByTerm(1n, thirds, "2026-11-30")).toBeNull();
    expect(splitByTerm(5000n, [{ days: 0, percent: "x" }], "2026-11-30")).toBeNull();
  });

  it("counts days on the calendar", () => {
    expect(addDays("2026-11-06", 0)).toBe("2026-11-06");
    expect(addDays("2026-11-06", 60)).toBe("2027-01-05");
    expect(addDays("2027-12-31", 1)).toBe("2028-01-01");
    expect(addDays("2028-02-28", 1)).toBe("2028-02-29");
  });
});
