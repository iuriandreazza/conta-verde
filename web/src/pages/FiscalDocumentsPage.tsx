import { useCallback, useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type FiscalDocument, type FiscalDocumentFilters, type FiscalEvent, type OperationNature } from "../api";
import { Button } from "../components/Button";
import { FiscalDocumentSheet } from "../components/FiscalDocumentSheet";
import { IssueForm } from "../components/FiscalIssueForm";
import { ServiceIssueForm } from "../components/FiscalServiceIssueForm";
import { HelpTitle } from "../components/HelpTitle";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Sheet } from "../components/Sheet";
import { Spinner } from "../components/Spinner";
import { useT } from "../i18n";
import { formatMoney } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";
import { EVENT_STATUS_LABELS, KIND_LABELS, STATUS_LABELS, documentLabel } from "../lib/fiscalDocuments";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

/** NF-e and NFS-e issued from sales, what the authority said about each, and the numbers declared unused (ADR 0076, ADR 0081). */
export function FiscalDocumentsPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("fiscal");
  const [filters, setFilters] = useState<FiscalDocumentFilters>({ status: "" });
  const [natures, setNatures] = useState<OperationNature[]>([]);
  const [issuing, setIssuing] = useState<"nfe" | "nfse" | null>(null);
  const [shown, setShown] = useState<string | null>(null);
  const [highlighted, setHighlighted] = useState<string | null>(null);

  const list = usePagedList<FiscalDocument, FiscalDocumentFilters>(
    (applied, after) => api.listFiscalDocuments(companyId, applied, after),
    filters,
  );

  useEffect(() => {
    api
      .getFiscalSettings(companyId)
      .then((settings) => setNatures(settings?.natures ?? []))
      .catch(() => setNatures([]));
  }, [companyId]);

  // The list does not carry the files: the sheet loads the document itself, so it opens whatever the filter shows.
  function changed(document: FiscalDocument) {
    list.setItems((items) => items.map((item) => (item.id === document.id ? { ...document, files: [] } : item)));
  }

  return (
    <>
      <PageHeader
        title={t("Fiscal documents")}
        description={t(
          "The NF-e (products) and NFS-e (services) issued from completed sales, what the authority answered about each, the files to download, cancellation, NF-e correction letters and the invalidation of numbers. A sale with products and services has one document of each kind.",
        )}
        actions={
          canWrite && (
            <>
              <Button type="button" onClick={() => setIssuing("nfe")}>
                {t("Issue NF-e")}
              </Button>
              <Button type="button" onClick={() => setIssuing("nfse")}>
                {t("Issue NFS-e")}
              </Button>
            </>
          )
        }
      />
      <Notice message={list.error} />
      <div className="filters">
        <label>
          {t("Status")}
          <select value={filters.status} onChange={(event) => setFilters({ status: event.target.value as FiscalDocumentFilters["status"] })}>
            <option value="">{t("All")}</option>
            {Object.entries(STATUS_LABELS).map(([key, label]) => (
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
        <p className="empty">{t("No fiscal documents yet.")}</p>
      ) : (
        <table className="list-table">
          <thead>
            <tr>
              <th>{t("Type")}</th>
              <th>{t("Number")}</th>
              <th>{t("Recipient")}</th>
              <th>{t("Document issued on")}</th>
              <th>{t("Total")}</th>
              <th>{t("Status")}</th>
            </tr>
          </thead>
          <tbody>
            {list.items.map((item, index) => (
              <ListRow key={item.id} index={index} highlighted={item.id === highlighted} tone={item.status === "rejected" || item.status === "denied" ? "critical" : undefined}>
                <td>{KIND_LABELS[item.kind]}</td>
                <td>
                  <button type="button" className="link-button" onClick={() => setShown(item.id)}>
                    {documentLabel(item)}
                  </button>
                </td>
                <td>{item.recipient_name}</td>
                <td>{formatDate(item.issued_on)}</td>
                <td className="amount">{formatMoney(item.total)}</td>
                <td>
                  <span className="badge">{t(STATUS_LABELS[item.status])}</span>
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

      <Invalidations companyId={companyId} canWrite={canWrite} />

      <Sheet open={issuing === "nfe"} title={t("Issue NF-e")} onClose={() => setIssuing(null)}>
        <IssueForm
          companyId={companyId}
          natures={natures}
          submitLabel={t("Issue")}
          onSubmit={async (choices) => {
            const made = await api.issueFiscalDocument(companyId, choices);
            setIssuing(null);
            setHighlighted(made.id);
            await list.reload();
            setShown(made.id);
          }}
        />
      </Sheet>
      <Sheet open={issuing === "nfse"} title={t("Issue NFS-e")} onClose={() => setIssuing(null)}>
        <ServiceIssueForm
          companyId={companyId}
          natures={natures}
          submitLabel={t("Issue")}
          onSubmit={async (choices) => {
            const made = await api.issueServiceDocument(companyId, choices);
            setIssuing(null);
            setHighlighted(made.id);
            await list.reload();
            setShown(made.id);
          }}
        />
      </Sheet>
      <Sheet open={shown !== null} title={t("Fiscal document")} onClose={() => setShown(null)}>
        {shown && <OpenDocument companyId={companyId} id={shown} natures={natures} canWrite={canWrite} onChanged={changed} />}
      </Sheet>
    </>
  );
}

/** One document, read from the server when its sheet opens and kept as the actions on it answer. */
function OpenDocument({
  companyId,
  id,
  natures,
  canWrite,
  onChanged,
}: {
  companyId: string;
  id: string;
  natures: OperationNature[];
  canWrite: boolean;
  onChanged: (document: FiscalDocument) => void;
}) {
  const t = useT();
  const [document, setDocument] = useState<FiscalDocument | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    setDocument(null);
    setError(null);
    api
      .getFiscalDocument(companyId, id)
      .then((found) => current && setDocument(found))
      .catch((failure) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId, id]);

  if (error) return <Notice message={error} />;
  if (!document) {
    return (
      <p role="status" className="status">
        <Spinner /> {t("Loading…")}
      </p>
    );
  }
  return (
    <FiscalDocumentSheet
      companyId={companyId}
      document={document}
      natures={natures}
      canWrite={canWrite}
      onChanged={(changed) => {
        setDocument(changed);
        onChanged(changed);
      }}
    />
  );
}

/** The ranges of numbers declared unused, and the form to declare one. */
function Invalidations({ companyId, canWrite }: { companyId: string; canWrite: boolean }) {
  const t = useT();
  const [items, setItems] = useState<FiscalEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [form, setForm] = useState({ series: "1", first: "", last: "", reason: "" });
  const [pending, setPending] = useState(false);

  const load = useCallback(async () => {
    try {
      setItems(await api.listInvalidations(companyId));
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }, [companyId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const [series, first, last] = [form.series, form.first, form.last].map(Number);
    if (![series, first, last].every(Number.isInteger)) {
      setError(t("The series and the numbers are whole numbers."));
      return;
    }
    setPending(true);
    setError(null);
    try {
      await api.invalidateNumbers(companyId, { series, first, last, reason: form.reason });
      setForm({ ...form, first: "", last: "", reason: "" });
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setPending(false);
    }
    await load();
  }

  return (
    <section aria-label={t("Invalidation of numbers")}>
      <HelpTitle title={t("Invalidation of numbers")} level={2}>
        <p>
          {t(
            "Declares to SEFAZ that a range of numbers in the series will not be used (a gap). A number that one of our invoices already uses cannot be invalidated. The request is recorded before it is sent and, if the answer does not come, the system does not repeat it: check at the gateway and use Give up only if SEFAZ did not receive it.",
          )}
        </p>
      </HelpTitle>
      <Notice message={error} />
      {canWrite && (
        <form className="record-form" onSubmit={(event) => void submit(event)}>
          <div className="field-grid">
            <label>
              {t("Series")}
              <input required inputMode="numeric" value={form.series} onChange={(event) => setForm({ ...form, series: event.target.value })} />
            </label>
            <label>
              {t("From number")}
              <input required inputMode="numeric" value={form.first} onChange={(event) => setForm({ ...form, first: event.target.value })} />
            </label>
            <label>
              {t("To number")}
              <input required inputMode="numeric" value={form.last} onChange={(event) => setForm({ ...form, last: event.target.value })} />
            </label>
            <label>
              {t("Reason (15 to 255 characters)")}
              <input required minLength={15} maxLength={255} value={form.reason} onChange={(event) => setForm({ ...form, reason: event.target.value })} />
            </label>
          </div>
          <Button type="submit" pending={pending}>
            {t("Invalidate")}
          </Button>
        </form>
      )}
      {items.length > 0 && (
        <ul className="plain-list">
          {items.map((item) => (
            <li key={item.id}>
              {t("Series {series}, numbers {first} to {last} — {status}", {
                series: item.series ?? "",
                first: item.first ?? "",
                last: item.last ?? "",
                status: t(EVENT_STATUS_LABELS[item.status]),
              })}
              {item.refusal ? ` (${item.refusal.code}: ${item.refusal.message})` : ""}
              {canWrite && item.status === "sent" && (
                <Button
                  type="button"
                  onClick={() =>
                    void api
                      .abandonFiscalEvent(companyId, item.id)
                      .then(load)
                      .catch((failure) => setError(errorMessage(failure)))
                  }
                >
                  {t("Give up")}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
