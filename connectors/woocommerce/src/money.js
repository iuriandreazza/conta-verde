// Money in whole cents, as BigInt: a store's amounts are decimal strings and are never turned into a Number.

const ZERO = 0n;

/** "12.34", "12", "12.3" -> 1234n. At most two decimals (what a store sends with `dp=2`); anything else is refused. */
export function parseCents(text) {
  const match = /^(\d+)(?:\.(\d{1,2}))?$/.exec(String(text ?? "").trim());
  if (!match) throw new RangeError(`not an amount: ${JSON.stringify(text)}`);
  return BigInt(match[1]) * 100n + BigInt((match[2] ?? "").padEnd(2, "0") || "0");
}

/** 1234n -> "12.34". */
export function formatCents(cents) {
  const whole = cents / 100n;
  const fraction = (cents % 100n).toString().padStart(2, "0");
  return `${whole}.${fraction}`;
}

/** An amount in millionths (the API's six decimals) as a decimal string without trailing zeros: 12500000n -> "12.5". */
export function formatMicros(micros) {
  const whole = micros / 1_000_000n;
  const fraction = (micros % 1_000_000n).toString().padStart(6, "0").replace(/0+$/, "");
  return fraction === "" ? `${whole}` : `${whole}.${fraction}`;
}

const ceilDiv = (a, b) => (a + b - 1n) / b;
// Half away from zero, for amounts that are never negative.
const roundHalfUp = (a, b) => (a * 2n + b) / (b * 2n);

/**
 * One order line as the API takes it: a unit price of at most six decimals (rounded **up**) and a discount in cents that makes the
 * line worth exactly what the store charged (`total`). The ERP values a line as round_to_cents(quantity x price) - discount, so
 * the sale's total is the order's. `subtotal` is the line before coupons, `total` after them.
 */
export function lineOf({ quantity, subtotal, total }) {
  if (!Number.isInteger(quantity) || quantity < 1) throw new RangeError(`quantity must be a whole number: ${quantity}`);
  const before = parseCents(subtotal);
  const after = parseCents(total);
  if (after > before) throw new RangeError("the line total is above its subtotal");
  const q = BigInt(quantity);
  // Cents to millionths is x 10_000; the price is the smallest that does not go under the subtotal.
  const priceMicros = ceilDiv(before * 10_000n, q);
  const valueCents = roundHalfUp(q * priceMicros, 10_000n);
  const discount = valueCents - after;
  if (discount < ZERO) throw new RangeError("the line cannot be made to add up");
  return { unitPrice: formatMicros(priceMicros), discount: formatCents(discount), valueCents: after };
}
