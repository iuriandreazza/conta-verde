// Stock quantities are decimal strings with up to three decimals (a minimum may have more): compared as whole
// numbers in millionths in a `bigint`, never as a `Number` (ADR 0003).
const SCALE = 6;

function toMillionths(text: string): bigint | null {
  const match = /^(-?)(\d+)(?:\.(\d{1,6}))?$/.exec(text);
  if (!match) return null;
  const magnitude = BigInt(match[2]) * 10n ** BigInt(SCALE) + BigInt((match[3] ?? "").padEnd(SCALE, "0") || "0");
  return match[1] === "-" ? -magnitude : magnitude;
}

/** What is on hand is below zero: more was sold than came in. */
export function isNegative(quantity: string): boolean {
  const amount = toMillionths(quantity);
  return amount !== null && amount < 0n;
}

/** What is on hand is under the product's minimum (when it has one). */
export function isBelowMinimum(quantity: string, minimum: string | null): boolean {
  if (minimum === null) return false;
  const amount = toMillionths(quantity);
  const floor = toMillionths(minimum);
  return amount !== null && floor !== null && amount < floor;
}

/** A cost with six decimals shortened by dropping zeros at the end, never below two decimals: `6.000000` is `6.00`. */
export function trimCost(cost: string): string {
  return cost.replace(/^(\d+\.\d{2}\d*?)0+$/, "$1");
}

/** A signed quantity with its sign shown: `+10`, `-5`. */
export function signedQuantity(quantity: string): string {
  return quantity.startsWith("-") ? quantity : `+${quantity}`;
}
