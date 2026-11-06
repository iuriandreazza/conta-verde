import { toApiDecimal } from "./decimal";

// Amounts are decimal strings end to end (ADR 0003). To add or split them they are held as whole
// cents in a `bigint`: a `Number` would turn 0.1 + 0.2 into 0.30000000000000004, and the API takes
// thirteen digits before the point.

/**
 * What a person typed, in cents, or `null` when it is not an amount in cents: at most two decimals
 * (zeros after them are the same number), thirteen digits before the point, no sign.
 */
export function toCents(typed: string): bigint | null {
  const text = toApiDecimal(typed);
  const match = text === undefined ? null : /^(\d{1,13})(?:\.(\d{1,2})0*)?$/.exec(text);
  if (!match) return null;
  return BigInt(match[1]) * 100n + BigInt((match[2] ?? "").padEnd(2, "0"));
}

/** Cents as the plain decimal the API takes, without trailing zeros: `10050` as `100.5`. */
export function centsToApi(cents: bigint): string {
  const whole = cents / 100n;
  const fraction = String(cents % 100n).padStart(2, "0");
  return fraction === "00" ? String(whole) : `${whole}.${fraction.replace(/0$/, "")}`;
}

/** The sum of the amounts typed, in cents, or `null` while one of them is not an amount. */
export function sumCents(typed: string[]): bigint | null {
  let total = 0n;
  for (const text of typed) {
    const cents = toCents(text);
    if (cents === null) return null;
    total += cents;
  }
  return total;
}

/**
 * `count` shares of a total, each in cents, with the remainder on the last one, the rule of ADR 0003:
 * 100 in three is 33.33, 33.33 and 33.34. `null` when the total is not an amount or is zero, or
 * when there are no shares to make.
 */
export function splitAmount(typedTotal: string, count: number): string[] | null {
  const total = toCents(typedTotal);
  if (total === null || total === 0n || !Number.isInteger(count) || count < 1) return null;
  const parts = BigInt(count);
  const base = total / parts;
  const shares = Array<bigint>(count).fill(base);
  shares[count - 1] = total - base * (parts - 1n);
  // A share of zero would be refused by the API: the total is too small for that many installments.
  return shares.some((share) => share === 0n) ? null : shares.map(centsToApi);
}

const ISO_DATE = /^(\d{4})-(\d{2})-(\d{2})$/;

function daysIn(year: number, month: number): number {
  // Day 0 of the next month is the last day of this one.
  return new Date(Date.UTC(year, month, 0)).getUTCDate();
}

/** Whether the text is a real day written `YYYY-MM-DD`. */
export function isIsoDate(text: string): boolean {
  const match = ISO_DATE.exec(text);
  if (!match) return false;
  const [year, month, day] = [Number(match[1]), Number(match[2]), Number(match[3])];
  return month >= 1 && month <= 12 && day >= 1 && day <= daysIn(year, month);
}

/**
 * `count` dates a month apart starting at `first`, each counted from the first so that the 31st does
 * not slide to the 28th for good: 31 Jan, 28 Feb, 31 Mar. `null` when `first` is not a date.
 */
export function monthlyDates(first: string, count: number): string[] | null {
  const match = ISO_DATE.exec(first);
  if (!match || !isIsoDate(first) || !Number.isInteger(count) || count < 1) return null;
  const [year, month, day] = [Number(match[1]), Number(match[2]), Number(match[3])];
  return Array.from({ length: count }, (_, index) => {
    const months = year * 12 + (month - 1) + index;
    const nextYear = Math.floor(months / 12);
    const nextMonth = (months % 12) + 1;
    const nextDay = Math.min(day, daysIn(nextYear, nextMonth));
    return `${String(nextYear).padStart(4, "0")}-${String(nextMonth).padStart(2, "0")}-${String(nextDay).padStart(2, "0")}`;
  });
}

/** `2026-12-10` as `10/12/2026`. */
export function formatDate(iso: string): string {
  return iso.split("-").reverse().join("/");
}
