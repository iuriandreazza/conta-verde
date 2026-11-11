import { useEffect, useRef, useState, type ChangeEvent, type ReactNode } from "react";
import { errorMessage, type ImportNote, type ImportResult } from "../api";
import { useT } from "../i18n";
import { Button } from "./Button";
import { InfoTip } from "./InfoTip";
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
  /** What the file is, for the field ("CSV file" unless said), already in the person's language. */
  fileLabel?: string;
  /** The kinds of file the picker offers. */
  accept?: string;
  /** What an import makes, in words, singular and plural ("record created" / "records created" unless said), already in the person's language. */
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
  fileLabel,
  accept = ".csv,text/csv",
  created,
  blocked = false,
  run,
  onImported,
}: CsvImportProps) {
  const t = useT();
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
      <div className="card-title">
        <h2 id="import-title">{title}</h2>
        <InfoTip label={t("How to import this file")}>{children}</InfoTip>
      </div>
      <label>
        {fileLabel ?? t("CSV file")}
        <input
          key={inputKey}
          ref={input}
          type="file"
          accept={accept}
          disabled={pending !== null}
          onChange={choose}
        />
      </label>
      <div className="form-actions import-actions">
        <Button
          type="button"
          variant="ghost"
          pending={pending === "check"}
          disabled={blocked || file === null || pending === "import"}
          onClick={() => void send(true)}
        >
          {t("Check file")}
        </Button>
        <Button
          type="button"
          pending={pending === "import"}
          disabled={blocked || file === null || pending === "check"}
          onClick={() => void send(false)}
        >
          {t("Import")}
        </Button>
        {template !== undefined && (
          <a className="import-template" href={templateHref} download={templateName}>
            {t("Download template")}
          </a>
        )}
      </div>
      <Notice message={error} />
      {result && <ImportOutcome result={result} created={created ?? [t("record created"), t("records created")]} />}
    </section>
  );
}

function ImportOutcome({ result, created }: { result: ImportResult; created: [string, string] }) {
  const t = useT();
  const bad = result.refused.length > 0;
  const skipped = result.skipped.length;
  const skippedNote = skipped === 1 ? t("{count} row skipped", { count: skipped }) : t("{count} rows skipped", { count: skipped });
  const message = bad
    ? result.refused.length === 1
      ? t("Nothing was imported: {count} row has a problem. Fix the file and send it again.", { count: 1 })
      : t("Nothing was imported: {count} rows have a problem. Fix the file and send it again.", { count: result.refused.length })
    : result.dry_run
      ? result.valid === 1
        ? t("File is in order: {count} valid row. The ones already registered will be skipped.", { count: 1 })
        : t("File is in order: {count} valid rows. The ones already registered will be skipped.", { count: result.valid })
      : `${result.created} ${result.created === 1 ? created[0] : created[1]}${skipped > 0 ? `; ${skippedNote}` : ""}.`;
  return (
    <>
      <Notice message={message} tone={bad ? "warning" : "success"} />
      <NoteList title={t("With problems")} notes={result.refused} />
      <NoteList title={t("Skipped, already registered")} notes={result.skipped} />
    </>
  );
}

function NoteList({ title, notes }: { title: string; notes: ImportNote[] }) {
  const t = useT();
  if (notes.length === 0) {
    return null;
  }
  return (
    <div className="import-notes">
      <h3>{title}</h3>
      <ul>
        {notes.slice(0, MAX_LISTED_NOTES).map((note) => (
          <li key={`${note.row}-${note.message}`}>
            {t("Row {row}: {message}", { row: note.row, message: note.message })}
          </li>
        ))}
      </ul>
      {notes.length > MAX_LISTED_NOTES && (
        <p className="muted">{t("and {count} more.", { count: notes.length - MAX_LISTED_NOTES })}</p>
      )}
    </div>
  );
}
