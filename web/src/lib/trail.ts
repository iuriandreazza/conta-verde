import { formatMoney } from "./decimal";

/**
 * The `lines` of a trail entry as a person reads them (quote, sale, contract and purchase order trails store
 * them alike: one line per row as `product:quantity:price:discount:description`, the description last).
 */
export function linesTrailText(value: string, names: Map<string, string>): string {
  return value
    .split("\n")
    .map((line) => {
      // The description comes last and may hold colons of its own.
      const [product, quantity, price, discount, ...description] = line.split(":");
      const name = description.length > 0 ? description.join(":") : (names.get(product) ?? "item");
      return `${name} × ${quantity.replace(".", ",")} a ${formatMoney(price)}${discount === "0" ? "" : ` (− ${formatMoney(discount)})`}`;
    })
    .join("; ");
}
