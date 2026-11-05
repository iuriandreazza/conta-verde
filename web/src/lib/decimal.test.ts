import { describe, expect, it } from "vitest";
import { formatDecimal, formatMoney, toApiDecimal, toInputDecimal } from "./decimal";

describe("decimal text", () => {
  it("turns what a Brazilian types into the plain decimal the API takes", () => {
    expect(toApiDecimal("10,50")).toBe("10.50");
    expect(toApiDecimal("1.234,56")).toBe("1234.56");
    expect(toApiDecimal(" 10.5 ")).toBe("10.5");
    expect(toApiDecimal("   ")).toBeUndefined();
    // Not a decimal: left alone, so the API says so.
    expect(toApiDecimal("1e3")).toBe("1e3");
    expect(toApiDecimal("-1,5")).toBe("-1.5");
  });

  it("shows an edited value the way it is typed", () => {
    expect(toInputDecimal("12.5")).toBe("12,5");
    expect(toInputDecimal(null)).toBe("");
  });

  it("formats without ever going through a number", () => {
    expect(formatDecimal("1234.5", 2)).toBe("1.234,50");
    expect(formatDecimal("10")).toBe("10");
    expect(formatDecimal("0.000001")).toBe("0,000001");
    // 19 significant digits survive; a double would round them.
    expect(formatDecimal("9999999999999.999999")).toBe("9.999.999.999.999,999999");
    expect(formatMoney("12.5")).toBe("R$ 12,50");
    expect(formatMoney(null)).toBe("—");
  });
});
