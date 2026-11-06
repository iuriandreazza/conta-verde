/** Excel in Brazil reads a semicolon, and needs the byte-order mark to read accents as UTF-8. */
const SEPARATOR = ";";
const BYTE_ORDER_MARK = "﻿";

// A cell a spreadsheet would run as a formula; a decimal like -10.5 is a number, not one.
const FORMULA_START = /^(?:[=+@\t\r]|-(?!\d+(?:\.\d+)?$))/;

function cell(value: string): string {
  const safe = FORMULA_START.test(value) ? `'${value}` : value;
  return /[;"\r\n]/.test(safe) ? `"${safe.replaceAll('"', '""')}"` : safe;
}

/** The text of a CSV file: the header, then a line per row, in the same order of columns. */
export function toCsv(header: string[], rows: string[][]): string {
  return BYTE_ORDER_MARK + [header, ...rows].map((row) => row.map(cell).join(SEPARATOR)).join("\r\n") + "\r\n";
}

/** Hands the browser a file to save, as the person would from a link. */
export function downloadCsv(filename: string, text: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: "text/csv;charset=utf-8" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}
