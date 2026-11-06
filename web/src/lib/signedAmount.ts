import { centsToApi, toCents } from "./entryInstallments";

/**
 * What a person typed as an amount that may be negative (the balance of an account in overdraft), as
 * the plain decimal the API takes, or `null` when it is not an amount in cents. The digits follow the
 * rules of every other amount; only the leading minus is new.
 */
export function toSignedApi(typed: string): string | null {
  const text = typed.trim();
  const negative = text.startsWith("-");
  const cents = toCents(negative ? text.slice(1).trim() : text);
  if (cents === null) return null;
  return negative && cents !== 0n ? `-${centsToApi(cents)}` : centsToApi(cents);
}

/** `minuend` less `subtrahend`, both amounts in cents the API sent, as the decimal the page prints. */
export function difference(minuend: string, subtrahend: string): string {
  const cents = (toCents(minuend) ?? 0n) - (toCents(subtrahend) ?? 0n);
  return cents < 0n ? `-${centsToApi(-cents)}` : centsToApi(cents);
}

/** The sum of amounts that may be negative, as the decimal the page prints, or `null` while one is not an amount. */
export function sumSigned(typed: string[]): string | null {
  let total = 0n;
  for (const text of typed) {
    const negative = text.trim().startsWith("-");
    const cents = toCents(negative ? text.trim().slice(1) : text);
    if (cents === null) return null;
    total += negative ? -cents : cents;
  }
  return total < 0n ? `-${centsToApi(-total)}` : centsToApi(total);
}
