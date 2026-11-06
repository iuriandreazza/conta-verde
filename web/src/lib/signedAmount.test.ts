import { describe, expect, it } from "vitest";
import { difference, toSignedApi } from "./signedAmount";

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

describe("difference", () => {
  it("subtracts to the cent and keeps the sign", () => {
    expect(difference("200.25", "50.25")).toBe("150");
    expect(difference("50.25", "200.25")).toBe("-150");
    expect(difference("0.3", "0.1")).toBe("0.2");
    expect(difference("5", "5")).toBe("0");
  });
});
