import type { ImportNote, ImportResult } from "../api";

/**
 * How many rows go in one request: the API takes at most 2,000 and writes them one after the other,
 * so a request that is not too long answers before a proxy gives up on it.
 */
export const ENTRY_IMPORT_PIECE_ROWS = 1000;

type SendPiece = (csv: string, dryRun: boolean) => Promise<ImportResult>;

/** Rows the piece answered about, renumbered to where they are in the whole file. */
function shifted(notes: ImportNote[], offset: number): ImportNote[] {
  return notes.map((note) => ({ ...note, row: note.row + offset }));
}

/** The text of a CSV file as its header and its rows, the rows that have something in them. */
function split(text: string): { header: string; rows: string[] } {
  const [header = "", ...rows] = text.split(/\r?\n/);
  return { header, rows: rows.filter((row) => row.trim() !== "") };
}

/**
 * Imports an entries file in pieces of at most `ENTRY_IMPORT_PIECE_ROWS` rows, each with the header.
 * Every piece is checked before any is written, so a refused row anywhere writes nothing at all;
 * the pieces are then written in order and stop at the first failure, and since a row already
 * imported is skipped, sending the file again finishes what a failure left (ADR 0064). The rows in
 * the answer are numbered as in the whole file, header counted, as a spreadsheet shows them.
 */
export async function importEntriesInPieces(
  text: string,
  send: SendPiece,
  dryRun: boolean,
): Promise<ImportResult> {
  const { header, rows } = split(text);
  const pieces: { csv: string; offset: number }[] = [];
  for (let start = 0; start < rows.length || start === 0; start += ENTRY_IMPORT_PIECE_ROWS) {
    pieces.push({
      csv: [header, ...rows.slice(start, start + ENTRY_IMPORT_PIECE_ROWS)].join("\n"),
      offset: start,
    });
  }

  async function run(asDryRun: boolean): Promise<ImportResult> {
    const total: ImportResult = { dry_run: asDryRun, valid: 0, created: 0, skipped: [], refused: [] };
    for (const piece of pieces) {
      const answer = await send(piece.csv, asDryRun);
      total.valid += answer.valid;
      total.created += answer.created;
      total.skipped.push(...shifted(answer.skipped, piece.offset));
      total.refused.push(...shifted(answer.refused, piece.offset));
      if (answer.refused.length > 0 && !asDryRun) {
        break;
      }
    }
    return total;
  }

  const checked = await run(true);
  if (dryRun || checked.refused.length > 0) {
    return checked;
  }
  return run(false);
}
