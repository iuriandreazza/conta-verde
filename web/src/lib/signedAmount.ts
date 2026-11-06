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
