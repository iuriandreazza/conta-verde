import assert from "node:assert/strict";
import { test } from "node:test";
import { formatCents, formatMicros, lineOf, parseCents } from "../src/money.js";

test("amounts are read and written in whole cents, never as a Number", () => {
  assert.equal(parseCents("12.34"), 1234n);
  assert.equal(parseCents("12"), 1200n);
  assert.equal(parseCents("12.3"), 1230n);
  assert.equal(formatCents(1234n), "12.34");
  assert.equal(formatCents(5n), "0.05");
  for (const bad of ["", "abc", "-1.00", "1.234", "1,50", null]) assert.throws(() => parseCents(bad), RangeError, String(bad));
  assert.equal(formatMicros(12_500_000n), "12.5");
  assert.equal(formatMicros(3_333_334n), "3.333334");
  assert.equal(formatMicros(2_000_000n), "2");
});

// What the ERP does with a line, written independently with the same rule: round_to_cents(quantity x price) - discount, half up.
const erpValue = ({ quantity, unitPrice, discount }) => {
  const [whole, fraction = ""] = unitPrice.split(".");
  const micros = BigInt(whole) * 1_000_000n + BigInt(fraction.padEnd(6, "0"));
  const cents = (BigInt(quantity) * micros * 2n + 10_000n) / 20_000n;
  return cents - parseCents(discount);
};

test("a line is worth exactly what the store charged, whatever the quantity and the coupons", () => {
  const cases = [
    { quantity: 3, subtotal: "10.00", total: "10.00" },
    { quantity: 3, subtotal: "10.00", total: "9.00" },
    { quantity: 7, subtotal: "0.07", total: "0.07" },
    { quantity: 1, subtotal: "99.99", total: "50.00" },
    { quantity: 13, subtotal: "1234.56", total: "1000.00" },
    { quantity: 2, subtotal: "20.20", total: "0.00" },
  ];
  for (let q = 1; q <= 40; q++) for (const cents of [1, 99, 100, 101, 3333, 99999]) cases.push({ quantity: q, subtotal: formatCents(BigInt(cents)), total: formatCents(BigInt(cents)) });
  for (const line of cases) {
    const made = lineOf(line);
    assert.equal(erpValue({ quantity: line.quantity, unitPrice: made.unitPrice, discount: made.discount }), parseCents(line.total), JSON.stringify(line));
    assert.equal(made.valueCents, parseCents(line.total));
    assert.ok((made.unitPrice.split(".")[1] ?? "").length <= 6);
  }
});

test("a line that cannot be a line is refused", () => {
  assert.throws(() => lineOf({ quantity: 0, subtotal: "1.00", total: "1.00" }), RangeError);
  assert.throws(() => lineOf({ quantity: 1.5, subtotal: "1.00", total: "1.00" }), RangeError);
  assert.throws(() => lineOf({ quantity: 1, subtotal: "1.00", total: "2.00" }), /above its subtotal/);
});
