import { useEffect, useState } from "react";
import { api, errorMessage, type HistoryEntry } from "../api";
import { Notice } from "./Notice";
import { Spinner } from "./Spinner";

// The trail stores stable keys; people read these.
const FIELD_LABELS: Record<string, string> = {
  name: "Nome",
  document: "Documento",
  email: "E-mail",
  is_customer: "Cliente",
  is_supplier: "Fornecedor",
  document_check: "Verificação do CNPJ",
  registry_legal_name: "Razão social (Receita)",
  registry_situation: "Situação (Receita)",
};

const CHECK_LABELS: Record<string, string> = {
  verified: "Verificado",
  not_found: "Não encontrado",
  unverified: "Não verificado",
};

function label(field: string): string {
  return FIELD_LABELS[field] ?? field;
}

function display(field: string, value: string | null): string {
  if (value === null) return "—";
  if (field === "is_customer" || field === "is_supplier") return value === "true" ? "sim" : "não";
  if (field === "document_check") return CHECK_LABELS[value] ?? value;
  return value;
}

const formatTime = new Intl.DateTimeFormat("pt-BR", { dateStyle: "short", timeStyle: "short" });

/** Newest first, as a person scanning for "what changed last" expects. */
export function PartyHistory({ companyId, partyId }: { companyId: string; partyId: string }) {
  const [entries, setEntries] = useState<HistoryEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .partyHistory(companyId, partyId)
      .then((loaded) => setEntries([...loaded].reverse()))
      .catch((failure: unknown) => setError(errorMessage(failure)));
  }, [companyId, partyId]);

  if (error) {
    return <Notice message={error} />;
  }
  if (entries === null) {
    return (
      <p role="status" className="status">
        <Spinner size={16} /> Carregando histórico…
      </p>
    );
  }
  return (
    <ol className="history">
      {entries.map((entry) => (
        <li key={entry.sequence}>
          <p className="history-head">
            <strong>{entry.actor.name}</strong> {entry.action === "created" ? "cadastrou" : "alterou"}
            <time dateTime={new Date(entry.at).toISOString()}>
              {formatTime.format(new Date(entry.at))}
            </time>
          </p>
          <ul className="history-changes">
            {entry.changes.map((change) => (
              <li key={change.field}>
                <span className="muted">{label(change.field)}:</span>{" "}
                {entry.action === "created" || change.from === null ? (
                  display(change.field, change.to)
                ) : (
                  <>
                    {display(change.field, change.from)} → {display(change.field, change.to)}
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
