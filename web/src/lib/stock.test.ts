import { describe, expect, it } from "vitest";
import { isAmbiguousSigned, isBelowMinimum, isNegative, signedQuantity, toSignedDecimal, trimCost } from "./stock";

describe("stock quantities", () => {
  it("compares decimals exactly", () => {
    expect(isBelowMinimum("0.299", "0.3")).toBe(true);
    expect(isBelowMinimum("0.3", "0.3")).toBe(false);
    expect(isBelowMinimum("10", null)).toBe(false);
    expect(isNegative("-0.001")).toBe(true);
    expect(isNegative("0")).toBe(false);
  });

  it("shortens a cost only by dropping zeros, keeping two decimals", () => {
    expect(trimCost("6.000000")).toBe("6.00");
    expect(trimCost("3.330000")).toBe("3.33");
    expect(trimCost("3.333333")).toBe("3.333333");
    expect(trimCost("0.100000")).toBe("0.10");
  });

  it("shows the sign of a movement", () => {
    expect(signedQuantity("10")).toBe("+10");
    expect(signedQuantity("-5")).toBe("-5");
  });
});

describe("typed stock decimals", () => {
  it("keeps the minus of a signed quantity, with a comma or a dot", () => {
    expect(toSignedDecimal("-1,5")).toBe("-1.5");
    expect(toSignedDecimal("- 1.5")).toBe("-1.5");
    expect(toSignedDecimal("2,5")).toBe("2.5");
    expect(toSignedDecimal("  ")).toBeUndefined();
  });

  it("refuses a dotted figure that could be thousands, signed or not", () => {
    expect(isAmbiguousSigned("1.500")).toBe(true);
    expect(isAmbiguousSigned("-1.500")).toBe(true);
    expect(isAmbiguousSigned("1,500")).toBe(false);
    expect(isAmbiguousSigned("0.500")).toBe(false);
    expect(isAmbiguousSigned("")).toBe(false);
  });
});
