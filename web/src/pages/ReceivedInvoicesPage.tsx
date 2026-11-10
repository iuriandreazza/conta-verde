import { useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type Manifestation, type ReceivedInvoice, type ReceivedInvoiceFilters } from "../api";
import { Button } from "../components/Button";
import { HelpTitle } from "../components/HelpTitle";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Sheet } from "../components/Sheet";
import { Spinner } from "../components/Spinner";
import { formatMoney } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

export const MANIFESTATION_LABELS: Record<Manifestation, string> = {
  none: "Sem manifestação",
  awareness: "Ciência da operação",
  confirmation: "Confirmação da operação",
  unknown: "Desconhecimento da operação",
  not_performed: "Operação não realizada",
};

const SITUATION_LABELS: Record<ReceivedInvoice["situation"], string> = {
  authorized: "Autorizada",
  cancelled: "Cancelada",
  denied: "Denegada",
};

/** What may follow what the recipient has already declared: awareness first, then at most one final word. */
function wordsAfter(current: Manifestation): Array<Exclude<Manifestation, "none">> {
  if (current === "none") return ["awareness", "confirmation", "unknown", "not_performed"];
  if (current === "awareness") return ["confirmation", "unknown", "not_performed"];
  return [];
}

/**
 * The invoices other companies issued against this one, fetched from the gateway, and what the company declares to the authority
 * about each (the recipient's manifestation, ADR 0083). The list is a mirror: it moves when the gateway says so; the person's
 * words are the acts, each recorded in their name.
 */
export function ReceivedInvoicesPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("fiscal");
  const [filters, setFilters] = useState<ReceivedInvoiceFilters>({ manifestation: "" });
  const [answering, setAnswering] = useState<ReceivedInvoice | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const list = usePagedList<ReceivedInvoice, ReceivedInvoiceFilters>(
    (applied, after) => api.listReceivedInvoices(companyId, applied, after),
    filters,
  );

  async function sync() {
    setSyncing(true);
    setError(null);
    setMessage(null);
    try {
      const { added } = await api.syncReceivedInvoices(companyId);
      setMessage(added === 0 ? "Nenhuma nota nova." : `${added} ${added === 1 ? "nota nova" : "notas novas"}.`);
      await list.reload();
    } catch (failure) {
      setError(errorMessage(failure));
      // The passes before the one that failed already brought invoices in.
      await list.reload();
    } finally {
      setSyncing(false);
    }
  }

  return (
    <>
      <PageHeader
        title="Notas recebidas"
        description="As NF-e que outras empresas emitiram contra a sua, buscadas no gateway. A lista é um espelho do que o gateway diz; o que você declara à autoridade (ciência, confirmação, desconhecimento ou operação não realizada) é um ato seu e fica no histórico com o seu nome. O gateway também avisa quando há nota nova, mas o aviso só faz o sistema perguntar."
        actions={
          canWrite && (
            <Button type="button" pending={syncing} onClick={() => void sync()}>
              Buscar notas
            </Button>
          )
        }
      />
      <Notice message={error ?? list.error} />
      {message && <p role="status">{message}</p>}
      <div className="filters">
        <label>
          Manifestação
          <select value={filters.manifestation} onChange={(event) => setFilters({ manifestation: event.target.value as ReceivedInvoiceFilters["manifestation"] })}>
            <option value="">Todas</option>
            {Object.entries(MANIFESTATION_LABELS).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </select>
        </label>
      </div>
      {list.loading ? (
        <p role="status" className="status">
          <Spinner /> Carregando…
        </p>
      ) : list.items.length === 0 ? (
        <p className="empty">Nenhuma nota recebida ainda. Busque no gateway.</p>
      ) : (
        <table className="list-table">
          <thead>
            <tr>
              <th>Emitente</th>
              <th>Emitida em</th>
              <th>Total</th>
              <th>Situação</th>
              <th>Manifestação</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {list.items.map((item, index) => (
              <ListRow key={item.id} index={index} highlighted={false} tone={item.situation === "authorized" ? undefined : "critical"}>
                <td>
                  {item.issuer_name}
                  <small className="access-key"> {item.access_key}</small>
                </td>
                <td>{formatDate(item.issued_on)}</td>
                <td className="amount">{formatMoney(item.total)}</td>
                <td>
                  <span className="badge">{SITUATION_LABELS[item.situation]}</span>
                </td>
                <td>
                  <span className="badge">{MANIFESTATION_LABELS[item.manifestation]}</span>
                </td>
                <td>
                  {canWrite && item.situation === "authorized" && wordsAfter(item.manifestation).length > 0 && (
                    <Button type="button" onClick={() => setAnswering(item)}>
                      {`Manifestar ${item.issuer_name}`}
                    </Button>
                  )}
                </td>
              </ListRow>
            ))}
          </tbody>
        </table>
      )}
      {list.nextCursor !== null && (
        <Button type="button" pending={list.loadingMore} onClick={() => void list.loadMore()}>
          Carregar mais
        </Button>
      )}
      <Sheet open={answering !== null} title="Manifestação do destinatário" onClose={() => setAnswering(null)}>
        {answering && (
          <ManifestForm
            invoice={answering}
            // The list is read again, not patched: a filter keeps only what it names, and a refusal or a conflict means the
            // words offered came from a row that moved.
            onDeclared={async () => {
              setAnswering(null);
              await list.reload();
            }}
            onFailed={() => void list.reload()}
            companyId={companyId}
          />
        )}
      </Sheet>
    </>
  );
}

function ManifestForm({
  companyId,
  invoice,
  onDeclared,
  onFailed,
}: {
  companyId: string;
  invoice: ReceivedInvoice;
  onDeclared: (invoice: ReceivedInvoice) => Promise<void>;
  onFailed: () => void;
}) {
  const [word, setWord] = useState("");
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    try {
      await onDeclared(
        await api.manifestReceivedInvoice(companyId, invoice.id, word as Exclude<Manifestation, "none">, word === "not_performed" ? reason : undefined),
      );
    } catch (failure) {
      setError(errorMessage(failure));
      onFailed();
    } finally {
      setPending(false);
    }
  }

  return (
    <form className="record-form" onSubmit={(event) => void submit(event)}>
      <HelpTitle title="O que declarar" level={3}>
        <p>
          A ciência diz que você viu a nota e pode ser seguida de uma palavra final: confirmação, desconhecimento ou operação não
          realizada (esta pede um motivo). Depois da palavra final nada mais se declara. A autoridade pode recusar, e a recusa vem com o
          motivo dela.
        </p>
      </HelpTitle>
      <Notice message={error} />
      <p>
        {invoice.issuer_name} · {formatMoney(invoice.total)} · agora: {MANIFESTATION_LABELS[invoice.manifestation]}
      </p>
      <label>
        Palavra
        <select required value={word} onChange={(event) => setWord(event.target.value)}>
          <option value="">Escolha</option>
          {wordsAfter(invoice.manifestation).map((key) => (
            <option key={key} value={key}>
              {MANIFESTATION_LABELS[key]}
            </option>
          ))}
        </select>
      </label>
      {word === "not_performed" && (
        <label>
          Motivo (de 15 a 255 caracteres)
          <input required minLength={15} maxLength={255} value={reason} onChange={(event) => setReason(event.target.value)} />
        </label>
      )}
      <Button type="submit" pending={pending}>
        Declarar
      </Button>
    </form>
  );
}
