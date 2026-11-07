import { describe, expect, it } from "vitest";
import type { ImportResult } from "../api";
import { ENTRY_IMPORT_PIECE_ROWS, importEntriesInPieces } from "./entryImport";

const header = "reference;kind;description";

function file(rows: number): string {
  return [header, ...Array.from({ length: rows }, (_, index) => `r${index};payable;d${index}`)].join("\n");
}

function answering(refusedRows: Record<number, string> = {}) {
  const calls: { rows: number; dryRun: boolean; firstLine: string }[] = [];
  const send = async (csv: string, dryRun: boolean): Promise<ImportResult> => {
    const lines = csv.split("\n");
    calls.push({ rows: lines.length - 1, dryRun, firstLine: lines[0] });
    const refused = Object.entries(refusedRows)
      .filter(([row]) => Number(row) <= lines.length)
      .map(([row, message]) => ({ row: Number(row), message }));
    return { dry_run: dryRun, valid: lines.length - 1, created: dryRun ? 0 : lines.length - 1, skipped: [], refused };
  };
  return { calls, send };
}

describe("importing entries in pieces", () => {
  it("sends a small file as one request, checked before it is written", async () => {
    const { calls, send } = answering();
    const result = await importEntriesInPieces(file(3), send, false);

    expect(calls).toEqual([
      { rows: 3, dryRun: true, firstLine: header },
      { rows: 3, dryRun: false, firstLine: header },
    ]);
    expect(result).toMatchObject({ dry_run: false, created: 3, refused: [] });
  });

  it("only checks the file when asked to, whatever its size", async () => {
    const { calls, send } = answering();
    const result = await importEntriesInPieces(file(ENTRY_IMPORT_PIECE_ROWS + 5), send, true);

    expect(calls.map((call) => [call.rows, call.dryRun])).toEqual([
      [ENTRY_IMPORT_PIECE_ROWS, true],
      [5, true],
    ]);
    expect(result).toMatchObject({ dry_run: true, valid: ENTRY_IMPORT_PIECE_ROWS + 5, created: 0 });
  });

  it("checks every piece before writing any, then writes them in order with the header", async () => {
    const { calls, send } = answering();
    const result = await importEntriesInPieces(file(ENTRY_IMPORT_PIECE_ROWS * 2 + 1), send, false);

    expect(calls.map((call) => [call.rows, call.dryRun])).toEqual([
      [ENTRY_IMPORT_PIECE_ROWS, true],
      [ENTRY_IMPORT_PIECE_ROWS, true],
      [1, true],
      [ENTRY_IMPORT_PIECE_ROWS, false],
      [ENTRY_IMPORT_PIECE_ROWS, false],
      [1, false],
    ]);
    expect(calls.every((call) => call.firstLine === header)).toBe(true);
    expect(result.created).toBe(ENTRY_IMPORT_PIECE_ROWS * 2 + 1);
  });

  it("writes nothing when a row in a later piece is refused, and numbers it as in the whole file", async () => {
    // Row 2 of the second piece is row ENTRY_IMPORT_PIECE_ROWS + 2 of the file, header counted.
    const { calls, send } = answering({ 2: "no active category" });
    const refusedInSecond = async (csv: string, dryRun: boolean) => {
      const answer = await send(csv, dryRun);
      const second = csv.includes(`r${ENTRY_IMPORT_PIECE_ROWS};`);
      return second ? answer : { ...answer, refused: [] };
    };
    const result = await importEntriesInPieces(file(ENTRY_IMPORT_PIECE_ROWS + 3), refusedInSecond, false);

    expect(calls.every((call) => call.dryRun)).toBe(true);
    expect(result.refused).toEqual([{ row: ENTRY_IMPORT_PIECE_ROWS + 2, message: "no active category" }]);
    expect(result.created).toBe(0);
  });

  it("ignores blank lines, which the API does not count as rows either", async () => {
    const { calls, send } = answering();
    await importEntriesInPieces(`${header}\n\nr1;payable;d1\r\n\nr2;payable;d2\n\n`, send, true);

    expect(calls).toEqual([{ rows: 2, dryRun: true, firstLine: header }]);
  });
});
