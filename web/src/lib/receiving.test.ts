import { describe, expect, it } from "vitest";
import type { DocumentLine, PurchaseReceipt } from "../api";
import { leftByLine } from "./receiving";
import { quantityToApi } from "./saleLines";

const line = (quantity: string): DocumentLine => ({
  product_id: "pr1",
  description: "Item",
  quantity,
  unit_price: "1",
  discount: "0",
  total: quantity,
});

const receipt = (items: { line: number; quantity: string }[]): PurchaseReceipt => ({
  id: "r",
  purchase_id: "o",
  received_on: "2026-11-10",
  notes: null,
  items: items.map((item) => ({ ...item, value: "0" })),
  total: "0",
  payable_id: null,
});

describe("quantityToApi", () => {
  it("writes thousandths as decimal text without trailing zeros", () => {
    expect(quantityToApi(0n)).toBe("0");
    expect(quantityToApi(5n)).toBe("0.005");
    expect(quantityToApi(2500n)).toBe("2.5");
    expect(quantityToApi(10000n)).toBe("10");
  });
});

describe("leftByLine", () => {
  it("takes what each line received over every receipt off what was ordered, exactly", () => {
    const lines = [line("10"), line("0.3"), line("2")];
    const receipts = [receipt([{ line: 1, quantity: "4.5" }, { line: 2, quantity: "0.1" }]), receipt([{ line: 2, quantity: "0.2" }])];
    expect(leftByLine(lines, receipts)).toEqual(["5.5", "0", "2"]);
  });

  it("never goes below zero", () => {
    expect(leftByLine([line("1")], [receipt([{ line: 1, quantity: "3" }])])).toEqual(["0"]);
  });
});
