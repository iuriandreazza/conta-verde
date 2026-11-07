import type { Product } from "../api";
import { toCsv } from "./csv";
import { readCsv } from "./csvRead";
import { isAmbiguousSigned, toSignedDecimal } from "./stock";

const HEADER = ["SKU", "Produto", "Contado"];

/** The spreadsheet a count can be filled from: every product that can have stock, with the column to fill. */
export function countTemplate(products: Product[]): string {
  return toCsv(
    HEADER,
    products.filter((product) => product.active).map((product) => [product.sku ?? "", product.name, ""]),
  );
}

/** A header as people write it: without case, accents or spaces around it. */
const plain = (text: string) => text.normalize("NFD").replace(/\p{M}/gu, "").trim().toLowerCase();

const SKU_HEADERS = ["sku", "codigo", "código"].map(plain);
const COUNTED_HEADERS = ["contado", "quantidade", "contagem", "qtd"].map(plain);

export interface SheetLine {
  product_id: string;
  /** As typed in the sheet, for the form to check like anything typed. */
  counted: string;
}

/**
 * Reads a count out of a spreadsheet: a column of SKUs and a column of what was found (blank means not counted yet, so
 * the line is skipped). What cannot be used is said by line, so the person fixes the sheet or the form.
 */
export function readCountSheet(text: string, products: Product[]): { lines: SheetLine[]; problems: string[] } {
  const rows = readCsv(text);
  const header = rows[0]?.map(plain) ?? [];
  const skuAt = header.findIndex((name) => SKU_HEADERS.includes(name));
  const countedAt = header.findIndex((name) => COUNTED_HEADERS.includes(name));
  if (skuAt < 0 || countedAt < 0) {
    return { lines: [], problems: ["A primeira linha precisa ter as colunas SKU e Contado."] };
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
      problems.push(`Linha ${line}: não há produto com o SKU "${sku}".`);
    } else if (isAmbiguousSigned(counted)) {
      problems.push(`Linha ${line}: a quantidade "${counted}" é ambígua: escreva 1500 ou 1,5.`);
    } else if (!/^\d+(\.\d{1,3})?$/.test(toSignedDecimal(counted) ?? "")) {
      problems.push(`Linha ${line}: a quantidade "${counted}" precisa ser zero ou mais, com até 3 casas.`);
    } else {
      if (lines.has(product.id)) problems.push(`Linha ${line}: o SKU "${sku}" já apareceu: vale a última linha.`);
      lines.set(product.id, { product_id: product.id, counted });
    }
  }
  return { lines: [...lines.values()], problems };
}
