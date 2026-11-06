import { toApiDecimal } from "./decimal";

// A payment term applied to an amount, the way the API does it (ADR 0037): each installment is the total
// times its percentage cut to the cent, the last takes what is left, and each falls due `days` after the
// day of the sale. This is only the preview; what is stored is what the server works out.

/** `33.33` as 3333 (hundredths of a percent), or `null` when it is not a percentage with two decimals. */
function toHundredths(percent: string): bigint | null {
  const text = toApiDecimal(percent);
  const match = text === undefined ? null : /^(\d{1,3})(?:\.(\d{1,2}))?$/.exec(text);
  return match ? BigInt(match[1]) * 100n + BigInt((match[2] ?? "").padEnd(2, "0")) : null;
}

/** `days` after an ISO day, as one: counted on the calendar, not in hours. */
export function addDays(iso: string, days: number): string {
  const [year, month, day] = iso.split("-").map(Number);
  const moved = new Date(Date.UTC(year, month - 1, day + days));
  return moved.toISOString().slice(0, 10);
}

export interface TermInstallment {
  days: number;
  percent: string;
}

export interface Part {
  due: string;
  cents: bigint;
}

/** The installments of `totalCents` from the day `from`, or `null` when one would be less than a cent. */
export function splitByTerm(totalCents: bigint, term: TermInstallment[], from: string): Part[] | null {
  let left = totalCents;
  const parts: Part[] = [];
  for (const [index, installment] of term.entries()) {
    const hundredths = toHundredths(installment.percent);
    if (hundredths === null) return null;
    const cents = index === term.length - 1 ? left : (totalCents * hundredths) / 10_000n;
    if (cents <= 0n) return null;
    left -= cents;
    parts.push({ due: addDays(from, installment.days), cents });
  }
  return parts;
}
