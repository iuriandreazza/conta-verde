import { useState } from "react";
import { errorMessage } from "../api";
import { downloadCsv, toCsv } from "../lib/csv";
import { readCapped } from "../lib/readUpTo";
import { Button } from "./Button";
import { Notice } from "./Notice";

/** More than this is a job for a report, not for a file made in the browser. */
export const EXPORT_CAP = 2000;

interface ListToolsProps<T> {
  /** The file's name, without the extension. */
  filename: string;
  header: string[];
  /** One page of the list as it is filtered now. */
  fetchPage: (after?: string) => Promise<{ items: T[]; next_cursor: string | null }>;
  toRow: (item: T) => string[];
}

/**
 * "Exportar" and "Imprimir" for a list: the file holds every row the filters give (not just the pages
 * on screen), and the printed page is the list without the shell around it (see `@media print`).
 */
export function ListTools<T>({ filename, header, fetchPage, toRow }: ListToolsProps<T>) {
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState<{ text: string; tone: "error" | "warning" | "success" } | null>(null);

  async function exportRows() {
    setPending(true);
    setMessage(null);
    try {
      const { items, complete } = await readCapped(fetchPage, EXPORT_CAP);
      downloadCsv(`${filename}.csv`, toCsv(header, items.map(toRow)));
      setMessage(
        complete
          ? { text: `${items.length} ${items.length === 1 ? "linha exportada" : "linhas exportadas"}.`, tone: "success" }
          : { text: `Foram exportadas as primeiras ${EXPORT_CAP} linhas: filtre a lista para exportar o resto.`, tone: "warning" },
      );
    } catch (failure) {
      setMessage({ text: errorMessage(failure), tone: "error" });
    } finally {
      setPending(false);
    }
  }

  return (
    <>
      <Button type="button" variant="ghost" pending={pending} onClick={() => void exportRows()}>
        Exportar
      </Button>
      <Button type="button" variant="ghost" onClick={() => window.print()}>
        Imprimir
      </Button>
      <Notice message={message?.text ?? null} tone={message?.tone} />
    </>
  );
}
