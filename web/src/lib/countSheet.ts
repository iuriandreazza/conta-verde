import type { Product } from "../api";
import { DEFAULT_LANGUAGE, translate, type Translate } from "../i18n";
import { toCsv } from "./csv";
import { readCsv } from "./csvRead";
import { isAmbiguousSigned, toSignedDecimal } from "./stock";

// The functions below are called by tests and by code with no language at hand: they speak the default one.
const inDefaultLanguage: Translate = (key, params) => translate(DEFAULT_LANGUAGE, key, params);

/** The most of a sheet that is read: a count has at most a few hundred lines, so a bigger file is not one. */
export const MAX_SHEET_BYTES = 1024 * 1024;

/**
 * The spreadsheet a count can be filled from: every active product that has a SKU (the SKU is how a line finds its
 * product), with the column to fill.
 */
export function countTemplate(products: Product[], t: Translate = inDefaultLanguage): string {
  return toCsv(
    ["SKU", t("Product"), t("Counted")],
    products.filter((product) => product.active && product.sku !== null).map((product) => [product.sku ?? "", product.name, ""]),
  );
}

/** A header as people write it: without case, accents or spaces around it. */
const plain = (text: string) => text.normalize("NFD").replace(/\p{M}/gu, "").trim().toLowerCase();

// "counted" is the English template's header, so a template downloaded in either language reads back.
const SKU_HEADERS = ["sku", "codigo", "código"].map(plain);
const COUNTED_HEADERS = ["contado", "quantidade", "contagem", "qtd", "counted"].map(plain);

export interface SheetLine {
  product_id: string;
  /** As typed in the sheet, for the form to check like anything typed. */
  counted: string;
}

/**
 * Reads a count out of a spreadsheet: a column of SKUs and a column of what was found (blank means not counted yet, so
 * the line is skipped). What cannot be used is said by line, so the person fixes the sheet or the form.
 */
export function readCountSheet(text: string, products: Product[], t: Translate = inDefaultLanguage): { lines: SheetLine[]; problems: string[] } {
  const rows = readCsv(text);
  const header = rows[0]?.map(plain) ?? [];
  const skuAt = header.findIndex((name) => SKU_HEADERS.includes(name));
  const countedAt = header.findIndex((name) => COUNTED_HEADERS.includes(name));
  if (skuAt < 0 || countedAt < 0) {
    return { lines: [], problems: [t("The first row must have the SKU and Counted columns.")] };
  }
  const bySku = new Map(products.filter((product) => product.sku !== null).map((product) => [plain(product.sku as string), product]));
  const lines = new Map<string, SheetLine>();
  const problems: string[] = [];
  for (const [index, row] of rows.slice(1).entries()) {
    const line = index + 2;
    const sku = (row[skuAt] ?? "").trim();
    const counted = (row[countedAt] ?? "").trim();
    if (counted === "") continue;
    const product = bySku.get(plain(sku));
    if (sku === "" || product === undefined) {
      problems.push(t("Row {line}: there is no product with the SKU \"{sku}\".", { line, sku }));
    } else if (isAmbiguousSigned(counted)) {
      problems.push(t("Row {line}: the quantity \"{counted}\" is ambiguous: write 1500 or 1,5.", { line, counted }));
    } else if (!/^\d+(\.\d{1,3})?$/.test(toSignedDecimal(counted) ?? "")) {
      problems.push(t("Row {line}: the quantity \"{counted}\" must be zero or more, with up to 3 decimals.", { line, counted }));
    } else {
      if (lines.has(product.id)) problems.push(t("Row {line}: the SKU \"{sku}\" already appeared: the last row counts.", { line, sku }));
      lines.set(product.id, { product_id: product.id, counted });
    }
  }
  return { lines: [...lines.values()], problems };
}
