import { toApiDecimal } from "./decimal";
import { toCents } from "./entryInstallments";

// The lines of a quote, a sale or a contract, added up the way the API does (ADR 0037): a quantity in
// thousandths and a price in millionths of a real, both as whole numbers in a `bigint`, so that
// 1,5 x 3,33 is 4.995 exactly and rounds to the cent the same way on both sides.

const THOUSANDTHS = 3;
const MILLIONTHS = 6;
// A quantity times a price is in billionths of a real; a cent is 10^7 of them.
const BILLIONTHS_PER_CENT = 10_000_000n;

/** What a person typed as a whole number of 10^-`scale` units, or `null` when it has more decimals. */
function toScaled(typed: string, scale: number): bigint | null {
  const text = toApiDecimal(typed);
  const match = text === undefined ? null : /^(\d{1,13})(?:\.(\d{1,6}))?$/.exec(text);
  if (!match) return null;
  // Zeros after the last decimal that counts are the same number, and the API accepts them.
  const fraction = (match[2] ?? "").replace(/0+$/, "");
  if (fraction.length > scale) return null;
  return BigInt(match[1]) * 10n ** BigInt(scale) + BigInt(fraction.padEnd(scale, "0") || "0");
}

/** A quantity above 0 with at most three decimals, in thousandths. */
export function toQuantity(typed: string): bigint | null {
  const quantity = toScaled(typed, THOUSANDTHS);
  return quantity !== null && quantity > 0n ? quantity : null;
}

/** A unit price with at most six decimals, in millionths. */
export function toUnitPrice(typed: string): bigint | null {
  return toScaled(typed, MILLIONTHS);
}

/** Quantity times price in cents, rounded half up: the rule of ADR 0037. */
export function grossCents(quantity: bigint, unitPrice: bigint): bigint {
  return (quantity * unitPrice + BILLIONTHS_PER_CENT / 2n) / BILLIONTHS_PER_CENT;
}

export interface TypedLine {
  quantity: string;
  unitPrice: string;
  discount: string;
}

/** What a line is worth in cents, or `null` while it is not valid: the gross less the discount, never negative. */
export function lineTotalCents(line: TypedLine): bigint | null {
  const quantity = toQuantity(line.quantity);
  const price = toUnitPrice(line.unitPrice);
  const discount = line.discount.trim() === "" ? 0n : toCents(line.discount);
  if (quantity === null || price === null || discount === null) return null;
  const gross = grossCents(quantity, price);
  return discount > gross ? null : gross - discount;
}

/** The sum of the lines in cents, or `null` while one of them is not valid. */
export function linesTotalCents(lines: TypedLine[]): bigint | null {
  let total = 0n;
  for (const line of lines) {
    const cents = lineTotalCents(line);
    if (cents === null) return null;
    total += cents;
  }
  return total;
}
