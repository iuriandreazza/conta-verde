/**
 * Reads the text of a CSV file into rows of cells: a semicolon or a comma between cells (whichever the first line uses
 * more, since a Brazilian spreadsheet writes the first), cells in double quotes that may hold the separator, a line
 * break or a doubled quote, and a byte-order mark in front. Blank lines are left out.
 */
export function readCsv(text: string): string[][] {
  const source = text.startsWith("﻿") ? text.slice(1) : text;
  const firstLine = source.split(/\r?\n/, 1)[0] ?? "";
  const separator = firstLine.split(";").length >= firstLine.split(",").length ? ";" : ",";
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;

  const endCell = () => {
    row.push(cell);
    cell = "";
  };
  const endRow = () => {
    endCell();
    if (row.some((value) => value.trim() !== "")) rows.push(row);
    row = [];
  };

  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    if (quoted) {
      if (char === '"' && source[index + 1] === '"') {
        cell += '"';
        index += 1;
      } else if (char === '"') {
        quoted = false;
      } else {
        cell += char;
      }
    } else if (char === '"') {
      quoted = true;
    } else if (char === separator) {
      endCell();
    } else if (char === "\n" || char === "\r") {
      if (char === "\r" && source[index + 1] === "\n") index += 1;
      endRow();
    } else {
      cell += char;
    }
  }
  endRow();
  return rows;
}
