import { describe, expect, it } from "vitest";
import { toSignedApi } from "./signedAmount";

describe("toSignedApi", () => {
  it("reads amounts the way every other amount is read", () => {
    expect(toSignedApi("1.234,50")).toBe("1234.5");
    expect(toSignedApi("10")).toBe("10");
    expect(toSignedApi("0,05")).toBe("0.05");
  });

  it("keeps a minus sign, and drops it from zero", () => {
    expect(toSignedApi("-20,5")).toBe("-20.5");
    expect(toSignedApi("- 1.000,00")).toBe("-1000");
    expect(toSignedApi("-0,00")).toBe("0");
  });

  it("refuses what is not an amount in cents", () => {
    for (const typed of ["", "abc", "1,005", "--1", "-", "+5", "1e3"]) {
      expect(toSignedApi(typed), typed).toBeNull();
    }
  });
});
