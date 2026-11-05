import { useEffect, useState } from "react";
import { errorMessage, type HistoryEntry } from "../api";
import { Button } from "./Button";
import { Notice } from "./Notice";
import { Spinner } from "./Spinner";

const formatTime = new Intl.DateTimeFormat("pt-BR", { dateStyle: "short", timeStyle: "short" });

interface HistoryProps {
  /** Loads the trail, oldest first as the API sends it. Pass a stable function for one record. */
  load: () => Promise<HistoryEntry[]>;
  /** The trail stores stable keys; people read these. */
  fieldLabel: (field: string) => string;
  valueLabel: (field: string, value: string | null) => string;
  /** Shown for a record that was stored before the audit trail existed. */
  emptyMessage: string;
}

/** Newest first, as a person scanning for "what changed last" expects. */
export function History({ load, fieldLabel, valueLabel, emptyMessage }: HistoryProps) {
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Bumped by the retry button to load again.
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    // A reply that arrives after the inputs changed (or the panel closed) must not be shown.
    let current = true;
    setError(null);
    setEntries(null);
    load()
      .then((loaded) => current && setEntries([...loaded].reverse()))
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [load, attempt]);

  if (error) {
    return (
      <>
        <Notice message={error} />
        <Button type="button" variant="ghost" className="btn-small" onClick={() => setAttempt(attempt + 1)}>
          Tentar de novo
        </Button>
      </>
    );
  }
  if (entries === null) {
    return (
      <p role="status" className="status">
        <Spinner size={16} /> Carregando histórico…
      </p>
    );
  }
  if (entries.length === 0) {
    return <p className="muted">{emptyMessage}</p>;
  }
  return (
    <ol className="history">
      {entries.map((entry, index) => (
        // The list is read-only and replaced as a whole, so the position is a stable key.
        <li key={index}>
          <p className="history-head">
            <strong>{entry.actor.name}</strong> {entry.action === "created" ? "cadastrou" : "alterou"}
            <time dateTime={new Date(entry.at).toISOString()}>
              {formatTime.format(new Date(entry.at))}
            </time>
          </p>
          <ul className="history-changes">
            {entry.changes.map((change) => (
              <li key={change.field}>
                <span className="muted">{fieldLabel(change.field)}:</span>{" "}
                {entry.action === "created" || change.from === null ? (
                  valueLabel(change.field, change.to)
                ) : (
                  <>
                    {valueLabel(change.field, change.from)} → {valueLabel(change.field, change.to)}
                  </>
                )}
              </li>
            ))}
          </ul>
        </li>
      ))}
    </ol>
  );
}
