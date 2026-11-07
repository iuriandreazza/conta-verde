import { invoiceNumber } from "./invoice";

describe("invoiceNumber", () => {
  it("reads the number out of the access key without leading zeros", () => {
    // 2 state + 4 date + 14 issuer + 2 model + 3 series, then the number.
    const key = "35" + "2611" + "11222333000181" + "55" + "001" + "000001234" + "1" + "12345678" + "0";
    expect(key).toHaveLength(44);
    expect(invoiceNumber(key)).toBe("1234");
  });

  it("is empty when there is no key or it is not one", () => {
    expect(invoiceNumber(null)).toBe("");
    expect(invoiceNumber("123")).toBe("");
  });
});
