// Prices and quantities are decimal strings end to end (ADR 0003): a `Number` holds about 15
// significant digits and the API accepts 19, so nothing here ever converts one.

/**
 * What a person types, as the plain decimal the API takes: `10,50` and `1.234,56` are how a
 * Brazilian writes them, `10.5` is accepted as it is. A blank field is "not set". Anything else
 * is passed on untouched for the API to refuse with its own message.
 */
export function toApiDecimal(input: string): string | undefined {
  const text = input.trim();
  if (text === "") return undefined;
  return text.includes(",") ? text.replace(/\./g, "").replace(",", ".") : text;
}

/** The API's decimal as it is shown in a field to be edited: `12.5` as `12,5`. */
export function toInputDecimal(value: string | null): string {
  return value === null ? "" : value.replace(".", ",");
}

/** `1234.5` as `1.234,50`, with at least `minimumFractionDigits` after the comma. */
export function formatDecimal(value: string, minimumFractionDigits = 0): string {
  const [integer, fraction = ""] = value.split(".");
  const grouped = integer.replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  const digits = fraction.padEnd(minimumFractionDigits, "0");
  return digits === "" ? grouped : `${grouped},${digits}`;
}

export function formatMoney(value: string | null): string {
  return value === null ? "—" : `R$ ${formatDecimal(value, 2)}`;
}
