/**
 * The number of an NF-e as the invoice prints it, taken from its 44-character access key (the nine digits that follow the
 * model and the series), without leading zeros. Empty for an order that did not come from an invoice.
 */
export function invoiceNumber(accessKey: string | null): string {
  if (accessKey === null || accessKey.length !== 44) return "";
  return accessKey.slice(25, 34).replace(/^0+(?=\d)/, "");
}
