import { Fragment, useCallback, useState, type FormEvent } from "react";
import { Link, useParams } from "react-router-dom";
import {
  NO_QUOTE_FILTERS,
  api,
  errorMessage,
  type Sale,
  type Quote,
  type QuoteFilters,
  type QuoteInput,
  type QuoteStatus,
  type SentQuote,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { FileTextIcon } from "../components/icons/file-text";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { HandCoinsIcon } from "../components/icons/hand-coins";
import { SendIcon } from "../components/icons/send";
import { SquarePenIcon } from "../components/icons/square-pen";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { QuoteForm } from "../components/QuoteForm";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { msg, useT, type Translate } from "../i18n";
import { useLastValue } from "../lib/useLastValue";
import { useSalesTargets } from "../lib/useSalesTargets";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { treeRows } from "../lib/categoryTree";
import { formatMoney } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { QUOTE_STATUS_LABELS } from "../lib/quoteStatus";
import { todayIso } from "../lib/settlement";
import { linesTrailText } from "../lib/trail";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

const FIELD_LABELS: Record<string, string> = {
  customer_id: msg("Customer"),
  issued_on: msg("Issued on"),
  valid_until: msg("Valid until"),
  payment_term_id: msg("Payment term"),
  notes: msg("Notes"),
  lines: msg("Items"),
  status: msg("Status"),
  link: msg("Link"),
  answered_by: msg("Answered by"),
};

/** A quote is open to change while it is a draft or waiting for the answer. */
const isOpen = (quote: Quote) => quote.status === "draft" || quote.status === "sent";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function QuotesPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("sales");
  const [filters, setFilters] = useState<QuoteFilters>(NO_QUOTE_FILTERS);
  const [editing, setEditing] = useState<Quote | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);
  const [sendingFor, setSendingFor] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [sendError, setSendError] = useState<string | null>(null);
  const [sent, setSent] = useState<SentQuote | null>(null);
  const [convertingFor, setConvertingFor] = useState<string | null>(null);
  const [converting, setConverting] = useState(false);
  const [convertError, setConvertError] = useState<string | null>(null);
  const [converted, setConverted] = useState<Sale | null>(null);
  // What a quote points at is read once: the form needs it, and so do the names in the list.
  const targets = useSalesTargets(companyId);
  const { categories, costCenters, parties, names, error: targetsError, loaded: targetsLoaded } = targets;

  const { items, setItems, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<Quote, QuoteFilters>((applied, after) => api.listQuotes(companyId, applied, after), filters);
  const isFiltered = applied.q !== "" || applied.status !== "";

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A quote made from the top bar or from here: read the list again and flash the new row.
  useOnCreated("quote", (created) => {
    setSaved(null);
    setHighlightedId(created.id);
    void reload();
  });

  function closeSheet() {
    // A change on its way finishes first: closing now would hide what a failure has to say.
    if (saving) return;
    setEditing(null);
    setFormError(null);
  }

  async function handleSubmit(input: QuoteInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updateQuote(companyId, editing.id, input);
      // In place: the list keeps its page and its filters.
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setEditing(null);
      setSaved(
        editing.status === "sent" && updated.status === "draft"
          ? t("Quote no. {number} updated. It went back to a draft and the link sent stopped working: send it again.", { number: updated.number })
          : t("Quote no. {number} updated.", { number: updated.number }),
      );
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  async function handleConvert(event: FormEvent<HTMLFormElement>, quote: Quote) {
    event.preventDefault();
    if (converting) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (!isIsoDate(field("sold_on"))) {
      setConvertError(t("Enter the sale date."));
      return;
    }
    setConverting(true);
    setConvertError(null);
    try {
      const sale = await api.convertQuote(companyId, quote.id, {
        sold_on: field("sold_on"),
        category_id: field("category_id"),
        cost_center_id: field("cost_center_id") || undefined,
        seller_id: field("seller_id") || undefined,
      });
      setItems((current) => current.map((item) => (item.id === quote.id ? { ...item, status: "converted" } : item)));
      setConverted(sale);
    } catch (failure) {
      setConvertError(errorMessage(failure));
    } finally {
      setConverting(false);
    }
  }

  async function handleSend(event: FormEvent<HTMLFormElement>, quote: Quote) {
    event.preventDefault();
    if (sending) {
      return;
    }
    const to = String(new FormData(event.currentTarget).get("to") ?? "").trim();
    setSending(true);
    setSendError(null);
    try {
      const result = await api.sendQuote(companyId, quote.id, to);
      setItems((current) => current.map((item) => (item.id === result.quote.id ? result.quote : item)));
      setSent(result);
    } catch (failure) {
      setSendError(errorMessage(failure));
    } finally {
      setSending(false);
    }
  }

  const historyLoader = useCallback((id: string) => () => api.quoteHistory(companyId, id), [companyId]);

  return (
    <>
      <PageHeader
        title={t("Quotes")}
        description={t("Items with discount, validity and a link for the customer to approve. Once approved, it becomes a sale.")}
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("quote")}>
            {t("New quote")}
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
          <h2>{t("Issued")}</h2>
          <div className="filters" role="search">
            <label>
              {t("Search")}
              <input
                type="search"
                value={filters.q}
                placeholder={t("Number, item or notes")}
                autoComplete="off"
                maxLength={100}
                onChange={(event) => setFilters({ ...filters, q: event.target.value })}
              />
            </label>
            <label>
              {t("Status")}
              <select
                value={filters.status}
                onChange={(event) => setFilters({ ...filters, status: event.target.value as QuoteFilters["status"] })}
              >
                <option value="">{t("All")}</option>
                {Object.entries(QUOTE_STATUS_LABELS).map(([key, label]) => (
                  <option key={key} value={key}>
                    {t(label)}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <Notice message={error ?? targetsError} />
          {loading ? (
            <p role="status" className="status">
              <Spinner /> {t("Loading…")}
            </p>
          ) : items.length === 0 && error ? (
            <Button type="button" variant="ghost" onClick={() => void reload()}>
              {t("Try again")}
            </Button>
          ) : items.length === 0 && isFiltered ? (
            <div className="empty">
              <FileTextIcon size={40} aria-hidden />
              <p role="status">{t("No quotes found.")}</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_QUOTE_FILTERS })}>
                {t("Clear filters")}
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <FileTextIcon size={40} aria-hidden />
              <p>{t("No quotes yet.")}</p>
              <p className="muted">{t("Put the items together, send the link and the customer approves without needing an account.")}</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{t("No.")}</th>
                    <th>{t("Customer")}</th>
                    <th>{t("Validity")}</th>
                    <th className="num">{t("Total")}</th>
                    <th>{t("Status")}</th>
                    <th>
                      <span className="visually-hidden">{t("Actions")}</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((quote, index) => (
                    <Fragment key={quote.id}>
                      <ListRow index={index} highlighted={quote.id === highlightedId}>
                        <td>{quote.number}</td>
                        <td>
                          <div className="cell-title">{names.get(quote.customer_id) ?? ""}</div>
                          <div className="muted">
                            {quote.lines.length === 1 ? t("{count} item", { count: 1 }) : t("{count} items", { count: quote.lines.length })}
                          </div>
                        </td>
                        <td>{formatDate(quote.valid_until)}</td>
                        <td className="num">{formatMoney(quote.total)}</td>
                        <td>
                          <span className={statusClass(quote.status)}>{t(QUOTE_STATUS_LABELS[quote.status])}</span>
                        </td>
                        <td className="row-actions">
                          <a
                            className="btn btn-ghost btn-icon"
                            href={api.quotePdfUrl(companyId, quote.id)}
                            target="_blank"
                            rel="noreferrer"
                            aria-label={t("Quote {number} PDF", { number: quote.number })}
                            title={t("Quote {number} PDF", { number: quote.number })}
                          >
                            <FileTextIcon size={16} aria-hidden />
                          </a>
                          {canWrite && quote.status !== "declined" && quote.status !== "converted" && (
                            <Button
                              iconOnly
                              icon={HandCoinsIcon}
                              type="button"
                              variant="ghost"
                              aria-label={t("Convert quote {number} into a sale", { number: quote.number })}
                              aria-expanded={convertingFor === quote.id}
                              onClick={() => {
                                setConvertingFor(convertingFor === quote.id ? null : quote.id);
                                setConvertError(null);
                                setConverted(null);
                              }}
                            >
                              {t("Convert into a sale")}
                            </Button>
                          )}
                          {canWrite && isOpen(quote) && (
                            <>
                              <Button
                                iconOnly
                                icon={SquarePenIcon}
                                type="button"
                                variant="ghost"
                                aria-label={t("Change quote {number}", { number: quote.number })}
                                disabled={saving}
                                onClick={() => {
                                  setEditing(quote);
                                  setFormError(null);
                                  setSaved(null);
                                }}
                              >
                                {t("Change")}
                              </Button>
                              <Button
                                iconOnly
                                icon={SendIcon}
                                type="button"
                                variant="ghost"
                                aria-label={t("Send quote {number}", { number: quote.number })}
                                aria-expanded={sendingFor === quote.id}
                                onClick={() => {
                                  setSendingFor(sendingFor === quote.id ? null : quote.id);
                                  setSendError(null);
                                  setSent(null);
                                }}
                              >
                                {t("Send")}
                              </Button>
                            </>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={t("History of quote {number}", { number: quote.number })}
                            aria-expanded={historyOpenFor === quote.id}
                            aria-controls={`history-${quote.id}`}
                            icon={HistoryIcon}
                            onClick={() => setHistoryOpenFor(historyOpenFor === quote.id ? null : quote.id)}
                          >
                            {t("History")}
                          </Button>
                        </td>
                      </ListRow>
                      {convertingFor === quote.id && (
                        <tr>
                          <td colSpan={6} role="region" aria-label={t("Conversion of quote {number}", { number: quote.number })}>
                            {converted !== null && converted.quote_id === quote.id ? (
                              <Notice
                                tone="success"
                                message={t("Order no. {number} created from this quote. Complete it in Sales to generate the receivable.", { number: converted.number })}
                              />
                            ) : (
                              <form className="opening-form" onSubmit={(event) => void handleConvert(event, quote)}>
                                <label>
                                  {t("Sale date")}
                                  <input name="sold_on" type="date" required defaultValue={todayIso()} />
                                </label>
                                <label>
                                  {t("Revenue category")}
                                  <select name="category_id" required defaultValue="">
                                    <option value="" disabled>
                                      {t("Choose")}
                                    </option>
                                    {treeRows(categories)
                                      .filter(({ category }) => category.active && category.dre_group !== "transfers")
                                      .map(({ category, depth }) => (
                                        <option key={category.id} value={category.id}>
                                          {"— ".repeat(depth)}
                                          {category.name}
                                        </option>
                                      ))}
                                  </select>
                                </label>
                                <label>
                                  {t("Cost center")}
                                  <select name="cost_center_id" defaultValue="">
                                    <option value="">{t("None")}</option>
                                    {costCenters
                                      .filter((center) => center.active)
                                      .map((center) => (
                                        <option key={center.id} value={center.id}>
                                          {center.name}
                                        </option>
                                      ))}
                                  </select>
                                </label>
                                <label>
                                  {t("Seller")}
                                  <select name="seller_id" defaultValue="">
                                    <option value="">{t("None")}</option>
                                    {parties
                                      .filter((party) => party.is_seller)
                                      .map((party) => (
                                        <option key={party.id} value={party.id}>
                                          {party.name}
                                        </option>
                                      ))}
                                  </select>
                                </label>
                                <Button type="submit" icon={HandCoinsIcon} pending={converting}>
                                  {t("Create order")}
                                </Button>
                                <Notice message={convertError} />
                              </form>
                            )}
                            {converted !== null && converted.quote_id === quote.id && (
                              <p>
                                <Link to={`/companies/${companyId}/sales`}>{t("Go to sales")}</Link>
                              </p>
                            )}
                          </td>
                        </tr>
                      )}
                      {sendingFor === quote.id && (
                        <tr>
                          <td colSpan={6} role="region" aria-label={t("Sending of quote {number}", { number: quote.number })}>
                            <form className="opening-form" onSubmit={(event) => void handleSend(event, quote)}>
                              <label>
                                {t("Customer e-mail")}
                                <input
                                  name="to"
                                  type="email"
                                  required
                                  autoComplete="off"
                                  defaultValue={parties.find((party) => party.id === quote.customer_id)?.email ?? ""}
                                />
                              </label>
                              <Button type="submit" icon={SendIcon} pending={sending}>
                                {t("Send link")}
                              </Button>
                              <Notice message={sendError} />
                              {sent !== null && sent.quote.id === quote.id && (
                                <>
                                  <Notice
                                    tone={sent.emailed ? "success" : "warning"}
                                    message={
                                      sent.emailed
                                        ? t("Link queued to be sent by e-mail. It appears only here: copy it if you need it.")
                                        : t("The e-mail was not sent. Pass the link to the customer another way: it appears only here.")
                                    }
                                  />
                                  <input aria-label={t("Link for the customer")} readOnly value={sent.link} onFocus={(event) => event.target.select()} />
                                </>
                              )}
                            </form>
                          </td>
                        </tr>
                      )}
                      {historyOpenFor === quote.id && (
                        <tr id={`history-${quote.id}`}>
                          <td colSpan={6} role="region" aria-label={t("History of quote {number}", { number: quote.number })}>
                            <History
                              load={historyLoader(quote.id)}
                              fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)}
                              valueLabel={(field, value) => displayValue(field, value, names, t)}
                              emptyMessage={t("No records.")}
                            />
                          </td>
                        </tr>
                      )}
                    </Fragment>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {nextCursor !== null && (
            <Button type="button" variant="ghost" icon={ChevronDownIcon} pending={loadingMore} onClick={() => void loadMore()}>
              {t("Load more")}
            </Button>
          )}
        </section>
        {canWrite && (
          <Sheet open={editing !== null} locked={saving} title={t("Change quote {number}", { number: shown?.number ?? "" })} onClose={closeSheet}>
            {shown !== null &&
              (targetsLoaded ? (
                <QuoteForm
                  key={shown.id}
                  companyId={companyId}
                  quote={shown}
                  targets={targets}
                  pending={saving}
                  error={formError}
                  onSubmit={(input) => void handleSubmit(input)}
                  onCancel={closeSheet}
                />
              ) : targetsError === null ? (
                <p role="status" className="status">
                  <Spinner /> {t("Loading…")}
                </p>
              ) : (
                <p role="alert">{targetsError}</p>
              ))}
          </Sheet>
        )}
      </div>
    </>
  );
}

function statusClass(status: QuoteStatus): string {
  switch (status) {
    case "approved":
    case "converted":
      return "badge badge-dot badge-ok";
    case "declined":
      return "badge badge-dot badge-critical";
    case "sent":
      return "badge badge-dot badge-warning";
    default:
      return "badge badge-dot badge-muted";
  }
}

/** A value of the trail as a person reads it: names for references, words for keys. */
function displayValue(field: string, value: string | null, names: Map<string, string>, t: Translate): string {
  if (value === null) return "—";
  if (field === "customer_id" || field === "payment_term_id") return names.get(value) ?? value;
  if (field === "status") return t(QUOTE_STATUS_LABELS[value as QuoteStatus] ?? value);
  if (field === "issued_on" || field === "valid_until") return formatDate(value);
  if (field === "lines") return linesTrailText(value, names, t);
  return value;
}
