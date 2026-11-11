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
import { msg, useT } from "../i18n";
import { formatMoney } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

export const MANIFESTATION_LABELS: Record<Manifestation, string> = {
  none: msg("No manifestation"),
  awareness: msg("Acknowledgement of the operation"),
  confirmation: msg("Confirmation of the operation"),
  unknown: msg("Unawareness of the operation"),
  not_performed: msg("Operation not performed"),
};

const SITUATION_LABELS: Record<ReceivedInvoice["situation"], string> = {
  authorized: msg("Authorized"),
  cancelled: msg("Cancelled"),
  denied: msg("Denied"),
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
  const t = useT();
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
      setMessage(
        added === 0
          ? t("No new invoices.")
          : added === 1
            ? t("{count} new invoice.", { count: added })
            : t("{count} new invoices.", { count: added }),
      );
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
        title={t("Received invoices")}
        description={t(
          "The NF-e that other companies issued against yours, fetched from the gateway. The list is a mirror of what the gateway says; what you declare to the authority (acknowledgement, confirmation, unawareness or operation not performed) is your own act and stays in the history under your name. The gateway also warns when there is a new invoice, but the warning only makes the system ask.",
        )}
        actions={
          canWrite && (
            <Button type="button" pending={syncing} onClick={() => void sync()}>
              {t("Fetch invoices")}
            </Button>
          )
        }
      />
      <Notice message={error ?? list.error} />
      {message && <p role="status">{message}</p>}
      <div className="filters">
        <label>
          {t("Manifestation")}
          <select value={filters.manifestation} onChange={(event) => setFilters({ manifestation: event.target.value as ReceivedInvoiceFilters["manifestation"] })}>
            <option value="">{t("All")}</option>
            {Object.entries(MANIFESTATION_LABELS).map(([key, label]) => (
              <option key={key} value={key}>
                {t(label)}
              </option>
            ))}
          </select>
        </label>
      </div>
      {list.loading ? (
        <p role="status" className="status">
          <Spinner /> {t("Loading…")}
        </p>
      ) : list.items.length === 0 ? (
        <p className="empty">{t("No received invoices yet. Fetch them from the gateway.")}</p>
      ) : (
        <table className="list-table">
          <thead>
            <tr>
              <th>{t("Issuer")}</th>
              <th>{t("Document issued on")}</th>
              <th>{t("Total")}</th>
              <th>{t("Status")}</th>
              <th>{t("Manifestation")}</th>
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
                  <span className="badge">{t(SITUATION_LABELS[item.situation])}</span>
                </td>
                <td>
                  <span className="badge">{t(MANIFESTATION_LABELS[item.manifestation])}</span>
                </td>
                <td>
                  {canWrite && item.situation === "authorized" && wordsAfter(item.manifestation).length > 0 && (
                    <Button type="button" onClick={() => setAnswering(item)}>
                      {t("Declare on {name}", { name: item.issuer_name })}
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
          {t("Load more")}
        </Button>
      )}
      <Sheet open={answering !== null} title={t("Recipient's manifestation")} onClose={() => setAnswering(null)}>
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
  const t = useT();
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
      <HelpTitle title={t("What to declare")} level={3}>
        <p>
          {t(
            "Acknowledgement says you saw the invoice and can be followed by one final word: confirmation, unawareness or operation not performed (this one asks for a reason). After the final word nothing more is declared. The authority may refuse, and the refusal comes with its reason.",
          )}
        </p>
      </HelpTitle>
      <Notice message={error} />
      <p>
        {invoice.issuer_name} · {formatMoney(invoice.total)} · {t("now:")} {t(MANIFESTATION_LABELS[invoice.manifestation])}
      </p>
      <label>
        {t("Declaration")}
        <select required value={word} onChange={(event) => setWord(event.target.value)}>
          <option value="">{t("Choose")}</option>
          {wordsAfter(invoice.manifestation).map((key) => (
            <option key={key} value={key}>
              {t(MANIFESTATION_LABELS[key])}
            </option>
          ))}
        </select>
      </label>
      {word === "not_performed" && (
        <label>
          {t("Reason (15 to 255 characters)")}
          <input required minLength={15} maxLength={255} value={reason} onChange={(event) => setReason(event.target.value)} />
        </label>
      )}
      <Button type="submit" pending={pending}>
        {t("Declare")}
      </Button>
    </form>
  );
}
