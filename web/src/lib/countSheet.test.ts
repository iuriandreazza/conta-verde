import type { Product } from "../api";
import { countTemplate, readCountSheet } from "./countSheet";
import { readCsv } from "./csvRead";

const product = (id: string, name: string, sku: string | null, active = true) => ({ id, name, sku, active }) as Product;
const products = [product("p1", "Café", "CAFE-500"), product("p2", "Pão", "PAO-1"), product("p3", "Velho", "OLD", false), product("p4", "Sem código", null)];

describe("the count sheet", () => {
  it("makes a template of the active products, which reads back as an empty count", () => {
    const rows = readCsv(countTemplate(products));
    expect(rows[0]).toEqual(["SKU", "Produto", "Contado"]);
    expect(rows.slice(1)).toEqual([
      ["CAFE-500", "Café", ""],
      ["PAO-1", "Pão", ""],
      ["", "Sem código", ""],
    ]);
    expect(readCountSheet(countTemplate(products), products)).toEqual({ lines: [], problems: [] });
  });

  it("reads each counted line by SKU, whatever the case, accents and decimal comma, and skips what is blank", () => {
    const sheet = "Código;Quantidade\ncafe-500;12,5\nPAO-1;\nOLD;0\n";
    expect(readCountSheet(sheet, products)).toEqual({
      lines: [
        { product_id: "p1", counted: "12,5" },
        { product_id: "p3", counted: "0" },
      ],
      problems: [],
    });
  });

  it("says by line what it cannot use", () => {
    const sheet = "SKU;Contado\nNADA;3\nCAFE-500;1.500\nPAO-1;-2\nPAO-1;4\nPAO-1;5\n";
    const { lines, problems } = readCountSheet(sheet, products);
    expect(lines).toEqual([{ product_id: "p2", counted: "5" }]);
    expect(problems).toEqual([
      'Linha 2: não há produto com o SKU "NADA".',
      'Linha 3: a quantidade "1.500" é ambígua: escreva 1500 ou 1,5.',
      'Linha 4: a quantidade "-2" precisa ser zero ou mais, com até 3 casas.',
      'Linha 6: o SKU "PAO-1" já apareceu: vale a última linha.',
    ]);
  });

  it("wants the two columns in the first line", () => {
    expect(readCountSheet("A;B\n1;2\n", products).problems).toEqual(["A primeira linha precisa ter as colunas SKU e Contado."]);
  });
});
