import { useEffect, useRef, useState, type ChangeEvent, type ReactNode } from "react";
import { errorMessage, type ImportNote, type ImportResult } from "../api";
import { Button } from "./Button";
import { Notice } from "./Notice";

/** The rows listed for a file with many problems: the rest are counted. */
const MAX_LISTED_NOTES = 50;

interface CsvImportProps {
  title: string;
  /** What the columns of the file are, in words. */
  children: ReactNode;
  /** The header and one example row, which the person downloads and fills in; none for a file a bank makes. */
  template?: string;
  templateName?: string;
  /** What the file is, for the field: "Arquivo CSV" unless said. */
  fileLabel?: string;
  /** The kinds of file the picker offers. */
  accept?: string;
  /** What an import makes, in words: "cadastro criado" / "cadastros criados" unless said. */
  created?: [string, string];
  /** Holds the buttons back, because something the import needs is not chosen yet. */
  blocked?: boolean;
  /** Sends the file; with `dryRun` only checks it. */
  run: (file: File, dryRun: boolean) => Promise<ImportResult>;
  /** Called after rows were created, so the list shows them. */
  onImported: () => void;
}

/**
 * Imports a CSV file: check it first, then send it. Nothing is written while a row is refused, and
 * a row that is already registered is skipped, so the file can be fixed and sent again.
 */
export function CsvImport({
  title,
  children,
  template,
  templateName,
  fileLabel = "Arquivo CSV",
  accept = ".csv,text/csv",
  created = ["cadastro criado", "cadastros criados"],
  blocked = false,
  run,
  onImported,
}: CsvImportProps) {
  const [file, setFile] = useState<File | null>(null);
  // A new key empties the file input once the rows were imported.
  const [inputKey, setInputKey] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const [pending, setPending] = useState<"check" | "import" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);

  // Emptying the input remounts it, which drops the keyboard's place: put it back.
  useEffect(() => {
    if (inputKey > 0) {
      input.current?.focus();
    }
  }, [inputKey]);

  function choose(event: ChangeEvent<HTMLInputElement>) {
    setFile(event.target.files?.[0] ?? null);
    setResult(null);
    setError(null);
  }

  function forgetFile() {
    setFile(null);
    setInputKey((key) => key + 1);
  }

  async function send(dryRun: boolean) {
    if (file === null || pending !== null) {
      return;
    }
    setPending(dryRun ? "check" : "import");
    setError(null);
    setResult(null);
    try {
      const outcome = await run(file, dryRun);
      setResult(outcome);
      if (outcome.created > 0 || outcome.refused.length > 0) {
        // Imported, or to be fixed: either way the file is let go. A fixed file has to be chosen
        // again, which is also what makes the browser read it again.
        forgetFile();
      }
      if (outcome.created > 0) {
        onImported();
      }
    } catch (failure) {
      setError(errorMessage(failure));
      forgetFile();
    } finally {
      setPending(null);
    }
  }

  // A BOM makes a spreadsheet open the accents right.
  const templateHref = `data:text/csv;charset=utf-8,${encodeURIComponent(`﻿${template ?? ""}`)}`;

  return (
    <section className="card import-card" aria-labelledby="import-title">
      <h2 id="import-title">{title}</h2>
      <div className="muted">{children}</div>
      {template !== undefined && (
        <p>
          <a href={templateHref} download={templateName}>
            Baixar modelo
          </a>
        </p>
      )}
      <label>
        {fileLabel}
        <input
          key={inputKey}
          ref={input}
          type="file"
          accept={accept}
          disabled={pending !== null}
          onChange={choose}
        />
      </label>
      <div className="form-actions">
        <Button
          type="button"
          variant="ghost"
          pending={pending === "check"}
          disabled={blocked || file === null || pending === "import"}
          onClick={() => void send(true)}
        >
          Verificar arquivo
        </Button>
        <Button
          type="button"
          pending={pending === "import"}
          disabled={blocked || file === null || pending === "check"}
          onClick={() => void send(false)}
        >
          Importar
        </Button>
      </div>
      <Notice message={error} />
      {result && <ImportOutcome result={result} created={created} />}
    </section>
  );
}

function ImportOutcome({ result, created }: { result: ImportResult; created: [string, string] }) {
  const bad = result.refused.length > 0;
  const message = bad
    ? `Nada foi importado: ${plural(result.refused.length, "linha tem", "linhas têm")} problema. Corrija o arquivo e envie de novo.`
    : result.dry_run
      ? `Arquivo em ordem: ${plural(result.valid, "linha válida", "linhas válidas")}. As que já estiverem cadastradas serão ignoradas.`
      : `${plural(result.created, created[0], created[1])}${
          result.skipped.length > 0
            ? `; ${plural(result.skipped.length, "linha ignorada", "linhas ignoradas")}`
            : ""
        }.`;
  return (
    <>
      <Notice message={message} tone={bad ? "warning" : "success"} />
      <NoteList title="Com problema" notes={result.refused} />
      <NoteList title="Ignoradas, já cadastradas" notes={result.skipped} />
    </>
  );
}

function NoteList({ title, notes }: { title: string; notes: ImportNote[] }) {
  if (notes.length === 0) {
    return null;
  }
  return (
    <div className="import-notes">
      <h3>{title}</h3>
      <ul>
        {notes.slice(0, MAX_LISTED_NOTES).map((note) => (
          <li key={`${note.row}-${note.message}`}>
            Linha {note.row}: {note.message}
          </li>
        ))}
      </ul>
      {notes.length > MAX_LISTED_NOTES && (
        <p className="muted">e mais {notes.length - MAX_LISTED_NOTES}.</p>
      )}
    </div>
  );
}

function plural(count: number, one: string, many: string): string {
  return `${count} ${count === 1 ? one : many}`;
}
