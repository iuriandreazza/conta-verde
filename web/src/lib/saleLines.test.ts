import { describe, expect, it } from "vitest";
import { grossCents, lineTotalCents, linesTotalCents, toQuantity, toUnitPrice } from "./saleLines";

describe("the lines of a sale", () => {
  it("reads a quantity in thousandths and refuses what the API would", () => {
    expect(toQuantity("1,5")).toBe(1500n);
    expect(toQuantity("2")).toBe(2000n);
    expect(toQuantity("0,125")).toBe(125n);
    // Zeros that mean nothing are the same number.
    expect(toQuantity("1,5000")).toBe(1500n);
    expect(toQuantity("0")).toBeNull();
    expect(toQuantity("1,0001")).toBeNull();
    expect(toQuantity("")).toBeNull();
    expect(toQuantity("-1")).toBeNull();
  });

  it("reads a price in millionths", () => {
    expect(toUnitPrice("10,10")).toBe(10_100_000n);
    expect(toUnitPrice("0,000001")).toBe(1n);
    expect(toUnitPrice("0")).toBe(0n);
    expect(toUnitPrice("1,0000001")).toBeNull();
  });

  it("rounds the gross to the cent with halves going up, like the API", () => {
    // The worked cases of the domain's tests: 1.5 x 3.33 = 4.995 -> 5.00.
    expect(grossCents(1500n, 3_330_000n)).toBe(500n);
    expect(grossCents(1500n, 3_329_999n)).toBe(499n);
    expect(grossCents(3000n, 10_100_000n)).toBe(3030n);
    expect(grossCents(1000n, 5_000n)).toBe(1n);
    expect(grossCents(1000n, 4_999n)).toBe(0n);
  });

  it("values a line and a document, and says nothing while a line is not valid", () => {
    expect(lineTotalCents({ quantity: "3", unitPrice: "10,10", discount: "0,30" })).toBe(3000n);
    expect(lineTotalCents({ quantity: "1,5", unitPrice: "3,33", discount: "" })).toBe(500n);
    expect(lineTotalCents({ quantity: "1", unitPrice: "10", discount: "10" })).toBe(0n);
    expect(lineTotalCents({ quantity: "1", unitPrice: "10", discount: "10,01" })).toBeNull();
    expect(lineTotalCents({ quantity: "1", unitPrice: "10", discount: "0,001" })).toBeNull();
    expect(
      linesTotalCents([
        { quantity: "3", unitPrice: "10,10", discount: "0,30" },
        { quantity: "1,5", unitPrice: "3,33", discount: "" },
      ]),
    ).toBe(3500n);
    expect(linesTotalCents([{ quantity: "x", unitPrice: "1", discount: "" }])).toBeNull();
    expect(linesTotalCents([])).toBe(0n);
  });
});
