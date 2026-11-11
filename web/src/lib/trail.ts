import { DEFAULT_LANGUAGE, translate, type Translate } from "../i18n";
import { formatMoney } from "./decimal";

// A page that has not yet been given its translator (the purchases one) gets the text in the default language.
const inDefaultLanguage: Translate = (key, params) => translate(DEFAULT_LANGUAGE, key, params);

/**
 * The `lines` of a trail entry as a person reads them (quote, sale, contract and purchase order trails store
 * them alike: one line per row as `product:quantity:price:discount:description`, the description last).
 */
export function linesTrailText(value: string, names: Map<string, string>, t: Translate = inDefaultLanguage): string {
  return value
    .split("\n")
    .map((line) => {
      // The description comes last and may hold colons of its own.
      const [product, quantity, price, discount, ...description] = line.split(":");
      const name = description.length > 0 ? description.join(":") : (names.get(product) ?? t("item"));
      const amounts = { name, quantity: quantity.replace(".", ","), price: formatMoney(price), discount: formatMoney(discount) };
      return discount === "0" ? t("{name} × {quantity} at {price}", amounts) : t("{name} × {quantity} at {price} (− {discount})", amounts);
    })
    .join("; ");
}
