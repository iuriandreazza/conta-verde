import { toCsv } from "./csv";
import { readCsv } from "./csvRead";

describe("readCsv", () => {
  it("reads what toCsv writes, quotes, separators and line breaks inside a cell included", () => {
    const rows = [["a;b", 'say "hi"', "line\nbreak"], ["1,5", "", "x"]];
    expect(readCsv(toCsv(["h1", "h2", "h3"], rows))).toEqual([["h1", "h2", "h3"], ...rows]);
  });

  it("takes a comma when the first line uses commas, and ignores blank lines", () => {
    expect(readCsv("sku,contado\r\nA,1\r\n\r\nB,2\r\n")).toEqual([
      ["sku", "contado"],
      ["A", "1"],
      ["B", "2"],
    ]);
  });

  it("reads a decimal comma inside a semicolon file as one cell", () => {
    expect(readCsv("sku;contado\nA;1,5\n")).toEqual([
      ["sku", "contado"],
      ["A", "1,5"],
    ]);
  });
});
