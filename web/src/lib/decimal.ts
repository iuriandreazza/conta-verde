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
  // Dots are thousands separators only in the Brazilian shape. Anything else with a comma, like
  // `1,234.56` pasted from an English-language spreadsheet, goes through as typed and the API
  // refuses it: rewriting it would turn a thousand into a one without a word.
  return /^\d{1,3}(\.\d{3})*,\d+$|^\d+,\d+$/.test(text)
    ? text.replace(/\./g, "").replace(",", ".")
    : text;
}

/**
 * `1.234` is one thousand two hundred and thirty-four to a Brazilian and a little over one to the
 * API, and either reading would be stored without a sound. A number written with dots and no
 * comma that could be read as thousands is refused until the person says which one they meant.
 * A leading zero (`0.001`) cannot be thousands, so it is a plain decimal.
 */
export function isAmbiguousDecimal(input: string): boolean {
  return /^\s*[1-9]\d{0,2}(\.\d{3})+\s*$/.test(input);
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
