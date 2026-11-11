import { Fragment, useCallback, useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import {
  NO_SALE_FILTERS,
  api,
  errorMessage,
  type PaymentTerm,
  type Sale,
  type SaleFilters,
  type SaleInput,
  type SaleReturn,
  type SaleStatus,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HandCoinsIcon } from "../components/icons/hand-coins";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { RotateCCWIcon } from "../components/icons/rotate-ccw";
import { SquarePenIcon } from "../components/icons/square-pen";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { SaleForm } from "../components/SaleForm";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { msg, useI18n, useT, type Translate } from "../i18n";
import { useLastValue } from "../lib/useLastValue";
import { useSalesTargets } from "../lib/useSalesTargets";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { treeRows } from "../lib/categoryTree";
import { formatDecimal, formatMoney, toApiDecimal } from "../lib/decimal";
import { centsToApi, formatDate, isIsoDate, toCents } from "../lib/entryInstallments";
import { SALE_STATUS_LABELS } from "../lib/saleStatus";
import { todayIso } from "../lib/settlement";
import { splitByTerm } from "../lib/termSplit";
import { linesTrailText } from "../lib/trail";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

const FIELD_LABELS: Record<string, string> = {
  customer_id: msg("Customer"),
  seller_id: msg("Seller"),
  sold_on: msg("Sale date"),
  payment_term_id: msg("Payment term"),
  category_id: msg("Category"),
  cost_center_id: msg("Cost center"),
  notes: msg("Notes"),
  lines: msg("Items"),
  status: msg("Status"),
  quote_id: msg("Quote"),
};

type Panel = { kind: "complete" | "returns" | "history" | "cancel"; id: string };

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function SalesPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("sales");
  const [filters, setFilters] = useState<SaleFilters>(NO_SALE_FILTERS);
  const [editing, setEditing] = useState<Sale | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [panel, setPanel] = useState<Panel | null>(null);
  const [acting, setActing] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  // What a sale points at is read once: the form needs it, and so do the names in the list.
  const targets = useSalesTargets(companyId);
  const { terms, categories, names, error: targetsError, loaded: targetsLoaded } = targets;
  // What a return may be booked to: the categories in use that are not the transfers one.
  const returnCategories = treeRows(categories).filter(({ category }) => category.active && category.dre_group !== "transfers");

  const { items, setItems, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<Sale, SaleFilters>((applied, after) => api.listSales(companyId, applied, after), filters);
  const isFiltered = applied.q !== "" || applied.status !== "";

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // An order made from the top bar or from here: read the list again and flash the new row.
  useOnCreated("sale", (created) => {
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

  async function handleSubmit(input: SaleInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updateSale(companyId, editing.id, input);
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setEditing(null);
      setSaved(t("Order no. {number} updated.", { number: updated.number }));
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  async function act(sale: Sale, action: "complete" | "cancel") {
    if (acting) {
      return;
    }
    setActing(true);
    setActionError(null);
    setSaved(null);
    try {
      const updated =
        action === "complete" ? await api.completeSale(companyId, sale.id) : await api.cancelSale(companyId, sale.id);
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setPanel(null);
      setSaved(
        action === "complete"
          ? t("Sale no. {number} completed: the receivable is in the entries.", { number: updated.number })
          : sale.status === "completed"
            ? t("Sale no. {number} cancelled, and so was the receivable.", { number: updated.number })
            : t("Sale no. {number} cancelled.", { number: updated.number }),
      );
    } catch (failure) {
      setActionError(errorMessage(failure));
    } finally {
      setActing(false);
    }
  }

  const historyLoader = useCallback((id: string) => () => api.saleHistory(companyId, id), [companyId]);
  const toggle = (kind: Panel["kind"], id: string) => {
    setPanel(panel?.kind === kind && panel.id === id ? null : { kind, id });
    setActionError(null);
  };

  return (
    <>
      <PageHeader
        title={t("Sales")}
        description={t("Orders and sales: completing generates the receivable by the payment term; returns generate the refund payable.")}
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("sale")}>
            {t("New order")}
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
          <h2>{t("Orders and sales")}</h2>
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
                onChange={(event) => setFilters({ ...filters, status: event.target.value as SaleFilters["status"] })}
              >
                <option value="">{t("All")}</option>
                {Object.entries(SALE_STATUS_LABELS).map(([key, label]) => (
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
              <HandCoinsIcon size={40} aria-hidden />
              <p role="status">{t("No sales found.")}</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_SALE_FILTERS })}>
                {t("Clear filters")}
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <HandCoinsIcon size={40} aria-hidden />
              <p>{t("No sales yet.")}</p>
              <p className="muted">{t("Create an order here, or convert an approved quote.")}</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{t("No.")}</th>
                    <th>{t("Customer")}</th>
                    <th>{t("Date")}</th>
                    <th className="num">{t("Total")}</th>
                    <th>{t("Status")}</th>
                    <th>
                      <span className="visually-hidden">{t("Actions")}</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((sale, index) => (
                    <Fragment key={sale.id}>
                      <ListRow index={index} highlighted={sale.id === highlightedId}>
                        <td>{sale.number}</td>
                        <td>
                          <div className="cell-title">{names.get(sale.customer_id) ?? ""}</div>
                          <div className="muted">
                            {sale.lines.length === 1 ? t("{count} item", { count: 1 }) : t("{count} items", { count: sale.lines.length })}
                            {sale.seller_id !== null && ` · ${names.get(sale.seller_id) ?? ""}`}
                            {sale.contract_id !== null && ` · ${t("From a contract")}`}
                          </div>
                        </td>
                        <td>{formatDate(sale.sold_on)}</td>
                        <td className="num">{formatMoney(sale.total)}</td>
                        <td>
                          <span className={statusClass(sale.status)}>{t(SALE_STATUS_LABELS[sale.status])}</span>
                        </td>
                        <td className="row-actions">
                          {canWrite && sale.status === "open" && (
                            <>
                              <Button
                                iconOnly
                                icon={SquarePenIcon}
                                type="button"
                                variant="ghost"
                                aria-label={t("Change order {number}", { number: sale.number })}
                                disabled={saving || acting}
                                onClick={() => {
                                  setEditing(sale);
                                  setFormError(null);
                                  setSaved(null);
                                }}
                              >
                                {t("Change")}
                              </Button>
                              <Button
                                iconOnly
                                icon={CheckIcon}
                                type="button"
                                variant="ghost"
                                aria-label={t("Complete sale {number}", { number: sale.number })}
                                aria-expanded={panel?.kind === "complete" && panel.id === sale.id}
                                onClick={() => toggle("complete", sale.id)}
                              >
                                {t("Complete")}
                              </Button>
                            </>
                          )}
                          {canWrite && sale.status === "completed" && (
                            <Button
                              iconOnly
                              icon={RotateCCWIcon}
                              type="button"
                              variant="ghost"
                              aria-label={t("Returns of sale {number}", { number: sale.number })}
                              aria-expanded={panel?.kind === "returns" && panel.id === sale.id}
                              onClick={() => toggle("returns", sale.id)}
                            >
                              {t("Returns")}
                            </Button>
                          )}
                          {canWrite && sale.status !== "cancelled" && (
                            <Button
                              type="button"
                              variant="ghost"
                              className="btn-small"
                              aria-label={t("Cancel sale {number}", { number: sale.number })}
                              aria-expanded={panel?.kind === "cancel" && panel.id === sale.id}
                              onClick={() => toggle("cancel", sale.id)}
                            >
                              {t("Cancel")}
                            </Button>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={t("History of sale {number}", { number: sale.number })}
                            aria-expanded={panel?.kind === "history" && panel.id === sale.id}
                            icon={HistoryIcon}
                            onClick={() => toggle("history", sale.id)}
                          >
                            {t("History")}
                          </Button>
                        </td>
                      </ListRow>
                      {panel?.id === sale.id && panel.kind === "complete" && (
                        <tr>
                          <td colSpan={6} role="region" aria-label={t("Completion of sale {number}", { number: sale.number })}>
                            <CompletePanel
                              sale={sale}
                              term={terms.find((term) => term.id === sale.payment_term_id) ?? null}
                              acting={acting}
                              error={actionError}
                              onConfirm={() => void act(sale, "complete")}
                            />
                          </td>
                        </tr>
                      )}
                      {panel?.id === sale.id && panel.kind === "cancel" && (
                        <tr>
                          <td colSpan={6} role="region" aria-label={t("Cancellation of sale {number}", { number: sale.number })}>
                            <div className="settlements">
                              <p>
                                {sale.status === "completed"
                                  ? t("Cancelling the sale also cancels the receivable it generated, and this cannot be undone: to sell again, make another sale. If the receivable already has settlements, undo them first.")
                                  : t("Cancelling the order closes it: it is not reopened.")}
                              </p>
                              <Notice message={actionError} />
                              <div className="form-actions">
                                <Button type="button" pending={acting} onClick={() => void act(sale, "cancel")}>
                                  {t("Cancel the sale")}
                                </Button>
                                <Button type="button" variant="ghost" disabled={acting} onClick={() => setPanel(null)}>
                                  {t("Keep")}
                                </Button>
                              </div>
                            </div>
                          </td>
                        </tr>
                      )}
                      {panel?.id === sale.id && panel.kind === "returns" && (
                        <tr>
                          <td colSpan={6} role="region" aria-label={t("Returns of sale {number}", { number: sale.number })}>
                            <ReturnsPanel
                              companyId={companyId}
                              sale={sale}
                              categories={returnCategories.map(({ category, depth }) => ({ id: category.id, label: `${"— ".repeat(depth)}${category.name}` }))}
                              canWrite={canWrite}
                            />
                          </td>
                        </tr>
                      )}
                      {panel?.id === sale.id && panel.kind === "history" && (
                        <tr>
                          <td colSpan={6} role="region" aria-label={t("History of sale {number}", { number: sale.number })}>
                            <History
                              load={historyLoader(sale.id)}
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
          {panel === null && <Notice message={actionError} />}
        </section>
        {canWrite && (
          <Sheet open={editing !== null} locked={saving} title={t("Change order {number}", { number: shown?.number ?? "" })} onClose={closeSheet}>
            {shown !== null &&
              (targetsLoaded ? (
                <SaleForm
                  key={shown.id}
                  companyId={companyId}
                  sale={shown}
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

function statusClass(status: SaleStatus): string {
  switch (status) {
    case "completed":
      return "badge badge-dot badge-ok";
    case "cancelled":
      return "badge badge-dot badge-critical";
    default:
      return "badge badge-dot badge-warning";
  }
}

/** What billing the order will make: the installments the term gives, worked out as the server will. */
function CompletePanel({
  sale,
  term,
  acting,
  error,
  onConfirm,
}: {
  sale: Sale;
  term: PaymentTerm | null;
  acting: boolean;
  error: string | null;
  onConfirm: () => void;
}) {
  const { t, rich } = useI18n();
  const total = toCents(sale.total);
  // A term the page has not read is not "no term": the preview waits instead of showing what the server will not make.
  const waiting = sale.payment_term_id !== null && term === null;
  const parts =
    total === null || waiting
      ? null
      : term === null
        ? [{ due: sale.sold_on, cents: total }]
        : splitByTerm(total, term.installments, sale.sold_on);
  return (
    <div className="settlements">
      <p>
        {term === null
          ? rich("Completing generates a receivable of {amount}, due on the day of the sale. After that the items do not change.", {
              amount: <strong>{formatMoney(sale.total)}</strong>,
            })
          : rich("Completing generates a receivable of {amount} by the term {term}. After that the items do not change.", {
              amount: <strong>{formatMoney(sale.total)}</strong>,
              term: term.name,
            })}
      </p>
      {waiting ? (
        <p role="status" className="status">
          <Spinner /> {t("Loading the term…")}
        </p>
      ) : parts === null ? (
        <Notice message={t("The amount is too small for this term: an installment would be less than one cent.")} />
      ) : (
        <ul className="history">
          {parts.map((part, index) => (
            <li key={index}>
              {formatDate(part.due)} · {formatMoney(centsToApi(part.cents))}
            </li>
          ))}
        </ul>
      )}
      <Notice message={error} />
      <div className="form-actions">
        <Button type="button" icon={CheckIcon} pending={acting} disabled={parts === null} onClick={onConfirm}>
          {t("Complete sale")}
        </Button>
      </div>
    </div>
  );
}

/** What came back of a sale, and the form that records another return. */
function ReturnsPanel({
  companyId,
  sale,
  categories,
  canWrite,
}: {
  companyId: string;
  sale: Sale;
  categories: { id: string; label: string }[];
  canWrite: boolean;
}) {
  const t = useT();
  const [returns, setReturns] = useState<SaleReturn[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let current = true;
    api
      .listReturns(companyId, sale.id)
      .then((reply) => current && setReturns(reply.items))
      .catch((failure: unknown) => current && setLoadError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId, sale.id, version]);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    const items = sale.lines.flatMap((_, index) => {
      const quantity = toApiDecimal(field(`line-${index + 1}`));
      return quantity === undefined || quantity === "0" ? [] : [{ line: index + 1, quantity }];
    });
    if (items.length === 0) {
      setError(t("Enter the returned quantity of at least one item."));
      return;
    }
    if (!isIsoDate(field("returned_on")) || !isIsoDate(field("refund_due_on"))) {
      setError(t("Enter the return date and the due date of the refund."));
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.createReturn(companyId, sale.id, {
        returned_on: field("returned_on"),
        reason: field("reason") || undefined,
        items,
        refund_category_id: field("refund_category_id"),
        refund_due_on: field("refund_due_on"),
      });
      setVersion((current) => current + 1);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="settlements">
      <Notice message={loadError} />
      {returns === null ? (
        <p role="status" className="status">
          <Spinner /> {t("Loading…")}
        </p>
      ) : returns.length === 0 ? (
        <p className="muted">{t("Nothing was returned from this sale.")}</p>
      ) : (
        <ul className="history">
          {returns.map((made) => (
            <li key={made.id}>
              {t("{date} · refund of {amount} · {items}", {
                date: formatDate(made.returned_on),
                amount: formatMoney(made.refund),
                items: made.items
                  .map((item) => `${sale.lines[item.line - 1]?.description ?? t("item")} × ${formatDecimal(item.quantity)}`)
                  .join(", "),
              })}
              {made.reason !== null && ` · ${made.reason}`}
              {made.refund_entry_id === null && ` · ${t("refund not yet booked")}`}
            </li>
          ))}
        </ul>
      )}
      {canWrite && (
        <form key={version} className="settle-form" onSubmit={(event) => void submit(event)}>
          <h3>{t("Record a return")}</h3>
          {sale.lines.map((line, index) => (
            <label key={index}>
              {t("{name} (sold: {quantity})", { name: line.description, quantity: formatDecimal(line.quantity) })}
              <input
                name={`line-${index + 1}`}
                aria-label={t("Returned quantity of {name}", { name: line.description })}
                inputMode="decimal"
                autoComplete="off"
                placeholder="0"
              />
            </label>
          ))}
          <label>
            {t("Return date")}
            <input name="returned_on" type="date" required defaultValue={todayIso()} />
          </label>
          <label>
            {t("Reason")}
            <input name="reason" maxLength={200} autoComplete="off" />
          </label>
          <label>
            {t("Refund category")}
            <select name="refund_category_id" required defaultValue="">
              <option value="" disabled>
                {t("Choose")}
              </option>
              {categories.map((category) => (
                <option key={category.id} value={category.id}>
                  {category.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Refund due on")}
            <input name="refund_due_on" type="date" required defaultValue={todayIso()} />
          </label>
          <Notice message={error} />
          <Button type="submit" icon={PlusIcon} pending={saving}>
            {t("Record return")}
          </Button>
        </form>
      )}
    </div>
  );
}

/** A value of the trail as a person reads it: names for references, words for keys. */
function displayValue(field: string, value: string | null, names: Map<string, string>, t: Translate): string {
  if (value === null) return "—";
  if (["customer_id", "seller_id", "payment_term_id", "category_id", "cost_center_id"].includes(field)) {
    return names.get(value) ?? value;
  }
  if (field === "status") return t(SALE_STATUS_LABELS[value as SaleStatus] ?? value);
  if (field === "sold_on") return formatDate(value);
  if (field === "lines") return linesTrailText(value, names, t);
  return value;
}
