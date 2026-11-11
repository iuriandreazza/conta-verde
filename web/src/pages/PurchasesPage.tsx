import { Fragment, useCallback, useEffect, useState, type FormEvent } from "react";
import { Link, useParams } from "react-router-dom";
import {
  NO_PURCHASE_FILTERS,
  api,
  errorMessage,
  type PurchaseOrder,
  type PurchaseOrderFilters,
  type PurchaseOrderInput,
  type PurchaseReceipt,
  type PurchaseStatus,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { BoxIcon } from "../components/icons/box";
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { FilterTiles, type TileOption } from "../components/FilterTiles";
import { FileTextIcon } from "../components/icons/file-text";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { SendIcon } from "../components/icons/send";
import { SquarePenIcon } from "../components/icons/square-pen";
import { ColumnChooser } from "../components/ColumnChooser";
import { ListRow } from "../components/ListRow";
import { MailPanel } from "../components/MailPanel";
import { ListTools } from "../components/ListTools";
import { MenuButton } from "../components/MenuButton";
import { PeriodFilter } from "../components/PeriodFilter";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { PurchaseForm } from "../components/PurchaseForm";
import { Sheet } from "../components/Sheet";
import { Spinner } from "../components/Spinner";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { msg, useI18n, useT, type Translate } from "../i18n";
import { formatDecimal, formatMoney, toInputDecimal } from "../lib/decimal";
import { invoiceNumber } from "../lib/invoice";
import { useColumns, type ColumnDef } from "../lib/useColumns";
import { useCounts } from "../lib/useCounts";
import { formatDate, isIsoDate, wholeDate } from "../lib/entryInstallments";
import { PURCHASE_STATUS_LABELS, purchaseStatusClass } from "../lib/purchaseStatus";
import { leftByLine, receivedByLine } from "../lib/receiving";
import { quantityToApi, toQuantity } from "../lib/saleLines";
import { todayIso } from "../lib/settlement";
import { linesTrailText } from "../lib/trail";
import { useCanWrite } from "../lib/useCanWrite";
import { useLastValue } from "../lib/useLastValue";
import { usePagedList } from "../lib/usePagedList";
import { useSalesTargets } from "../lib/useSalesTargets";

const FIELD_LABELS: Record<string, string> = {
  supplier_id: msg("Supplier"),
  ordered_on: msg("Order date"),
  expected_on: msg("Expected delivery"),
  payment_term_id: msg("Payment term"),
  category_id: msg("Category"),
  cost_center_id: msg("Cost center"),
  notes: msg("Notes"),
  lines: msg("Items"),
  status: msg("Status"),
};

type PanelKind = "details" | "receive" | "approve" | "cancel" | "close" | "history" | "send";
type Panel = { kind: PanelKind; id: string };
type Action = "approve" | "cancel" | "close";

const ORDER_LABELS: Record<PurchaseOrderFilters["sort"], string> = {
  "-number": msg("Newest first"),
  number: msg("Oldest orders first"),
};

/** The statuses a bulk action applies to; the rest of what is ticked is left alone. */
const BULK_ELIGIBLE: Record<"approve" | "cancel", PurchaseStatus[]> = {
  approve: ["draft"],
  cancel: ["draft", "approved"],
};

const CONFIRMATIONS: Record<
  Action,
  { text: (t: Translate, order: PurchaseOrder) => string; button: string; done: (t: Translate, order: PurchaseOrder) => string }
> = {
  approve: {
    text: (t, order) =>
      t("Approving confirms the {total} order with the supplier: after that the items do not change.", { total: formatMoney(order.total) }),
    button: msg("Approve purchase"),
    done: (t, order) => t("Purchase no. {number} approved.", { number: order.number }),
  },
  cancel: {
    text: (t) => t("Cancelling the order closes it: it is not reopened, and it only works before anything is received."),
    button: msg("Cancel the purchase"),
    done: (t, order) => t("Purchase no. {number} cancelled.", { number: order.number }),
  },
  close: {
    text: (t) => t("Closing marks the order as received: what has not arrived is waived and will not be billed."),
    button: msg("Close the purchase"),
    done: (t, order) => t("Purchase no. {number} closed: what did not arrive was waived.", { number: order.number }),
  },
};

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
const PURCHASE_COLUMNS = [msg("Number"), msg("Supplier"), msg("Date"), msg("Expected"), msg("Status"), msg("Total")];

type PurchaseColumn = "ordered" | "expected" | "invoice" | "total";
const TABLE_COLUMNS: ColumnDef<PurchaseColumn>[] = [
  { key: "ordered", label: msg("Date") },
  { key: "expected", label: msg("Expected") },
  { key: "invoice", label: msg("Invoice no.") },
  { key: "total", label: msg("Total") },
];

export function PurchasesPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("purchases");
  const [filters, setFilters] = useState<PurchaseOrderFilters>(NO_PURCHASE_FILTERS);
  const [editing, setEditing] = useState<PurchaseOrder | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [panel, setPanel] = useState<Panel | null>(null);
  const [acting, setActing] = useState(false);
  // The orders ticked for a bulk action, by id: what is no longer in the list is not acted on.
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [actionError, setActionError] = useState<string | null>(null);
  // What an order points at is read once: the form needs it, and so do the names in the list.
  const targets = useSalesTargets(companyId);
  const { names, error: targetsError, loaded: targetsLoaded } = targets;

  const { items, setItems, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<PurchaseOrder, PurchaseOrderFilters>(
      (applied, after) => api.listPurchaseOrders(companyId, { ...applied, from: wholeDate(applied.from), to: wholeDate(applied.to) }, after),
      filters,
    );
  const columns = useColumns("purchases", TABLE_COLUMNS);
  const columnDefs = TABLE_COLUMNS.map((column) => ({ ...column, label: t(column.label) }));
  const selected = items.filter((order) => selectedIds.includes(order.id));
  // A tick on an order that left the list (another filter, another search) does not wait for it to come back.
  useEffect(() => {
    setSelectedIds((ids) => {
      const kept = ids.filter((id) => items.some((order) => order.id === id));
      return kept.length === ids.length ? ids : kept;
    });
  }, [items]);
  const span = 8 - columns.hiddenCount + (canWrite ? 1 : 0);
  const today = todayIso();
  const isFiltered = applied.q !== "" || applied.status !== "" || applied.from !== "" || applied.to !== "";
  const counts = useCounts(() => api.getPurchaseStatusCounts(companyId, { q: applied.q, from: wholeDate(applied.from), to: wholeDate(applied.to) }), items, !loading);
  const tiles: TileOption<PurchaseOrderFilters["status"]>[] = [
    { key: "", label: t("All"), count: counts === null ? null : Object.values(counts).reduce((sum, n) => sum + n, 0) },
    ...(Object.entries(PURCHASE_STATUS_LABELS) as [PurchaseStatus, string][]).map(([key, label]) => ({
      key,
      label: t(label),
      count: counts?.[key] ?? null,
    })),
  ];

  const replace = (updated: PurchaseOrder) =>
    setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // An order made from the top bar or from here: read the list again and flash the new row.
  const showCreated = (created: { id: string }) => {
    setSaved(null);
    setHighlightedId(created.id);
    void reload();
  };
  useOnCreated("purchase", showCreated);
  useOnCreated("purchaseImport", showCreated);

  function closeSheet() {
    // A change on its way finishes first: closing now would hide what a failure has to say.
    if (saving) return;
    setEditing(null);
    setFormError(null);
  }

  async function handleSubmit(input: PurchaseOrderInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updatePurchaseOrder(companyId, editing.id, input);
      replace(updated);
      setEditing(null);
      setSaved(t("Purchase no. {number} updated.", { number: updated.number }));
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  async function act(order: PurchaseOrder, action: Action) {
    if (acting) {
      return;
    }
    setActing(true);
    setActionError(null);
    setSaved(null);
    try {
      const call = { approve: api.approvePurchaseOrder, cancel: api.cancelPurchaseOrder, close: api.closePurchaseOrder }[action];
      const updated = await call(companyId, order.id);
      replace(updated);
      setPanel(null);
      setSaved(CONFIRMATIONS[action].done(t, updated));
    } catch (failure) {
      setActionError(errorMessage(failure));
    } finally {
      setActing(false);
    }
  }

  /** Does one action to every ordered ticked that it applies to, one after the other, and says how it went. */
  async function actOnSelected(action: "approve" | "cancel") {
    if (acting) return;
    const eligible = selected.filter((order) => BULK_ELIGIBLE[action].includes(order.status));
    setActing(true);
    setActionError(null);
    setSaved(null);
    const failed: string[] = [];
    let done = 0;
    for (const order of eligible) {
      try {
        const call = action === "approve" ? api.approvePurchaseOrder : api.cancelPurchaseOrder;
        replace(await call(companyId, order.id));
        done += 1;
      } catch (failure) {
        failed.push(t("no. {number} ({reason})", { number: order.number, reason: errorMessage(failure) }));
      }
    }
    setSaved(done === 0 ? null : bulkDone(action, done));
    if (failed.length > 0) {
      const failures = failed.join("; ");
      setActionError(action === "approve" ? t("Could not approve: {failures}.", { failures }) : t("Could not cancel: {failures}.", { failures }));
    }
    setSelectedIds([]);
    setActing(false);
  }

  function bulkDone(action: "approve" | "cancel", count: number): string {
    if (action === "approve") {
      return count === 1 ? t("{count} purchase approved.", { count }) : t("{count} purchases approved.", { count });
    }
    return count === 1 ? t("{count} purchase cancelled.", { count }) : t("{count} purchases cancelled.", { count });
  }

  /** A receipt changes the order's status, which only the order's own answer tells. */
  async function received(order: PurchaseOrder, receipt: PurchaseReceipt) {
    setPanel(null);
    setSaved(
      receipt.payable_id === null && receipt.total !== "0"
        ? t("Receipt recorded, but the payable has not been entered yet: finish the receipt in the purchase's details.")
        : receipt.payable_id === null
          ? t("Receipt recorded.")
          : t("Receipt recorded: the payable of {total} is in the entries.", { total: formatMoney(receipt.total) }),
    );
    try {
      replace(await api.getPurchaseOrder(companyId, order.id));
    } catch {
      await reload();
    }
  }

  const historyLoader = useCallback((id: string) => () => api.purchaseOrderHistory(companyId, id), [companyId]);
  const toggle = (kind: PanelKind, id: string) => {
    setPanel(panel?.kind === kind && panel.id === id ? null : { kind, id });
    setActionError(null);
  };
  const isOpen = (kind: PanelKind, id: string) => panel?.kind === kind && panel.id === id;

  return (
    <>
      <PageHeader
        title={t("Purchases")}
        description={t("Orders to suppliers: each receipt, even a partial one, makes the payable for what arrived, by the payment term.")}
      />
      <div className="list-actions">
        {canWrite && (
          <MenuButton
            label={t("New")}
            icon={PlusIcon}
            variant="primary"
            items={[
              { key: "purchase", label: t("New purchase"), icon: PlusIcon, onSelect: () => openCreate("purchase") },
              { key: "invoice", label: t("Enter NF-e (XML)"), icon: FileTextIcon, onSelect: () => openCreate("purchaseImport") },
            ]}
          />
        )}
        <ColumnChooser columns={columnDefs} isShown={columns.isShown} onToggle={columns.toggle} onReset={columns.reset} />
        <ListTools
          filename={t("purchases")}
          header={PURCHASE_COLUMNS.map((name) => t(name))}
          fetchPage={(after) => api.listPurchaseOrders(companyId, { ...applied, from: wholeDate(applied.from), to: wholeDate(applied.to) }, after)}
          toRow={(order) => [
            String(order.number),
            names.get(order.supplier_id) ?? "",
            order.ordered_on,
            order.expected_on ?? "",
            t(PURCHASE_STATUS_LABELS[order.status]),
            toInputDecimal(order.total),
          ]}
        />
      </div>
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
          <h2>{t("Purchase orders")}</h2>
          <FilterTiles
            label={t("Status")}
            value={filters.status}
            options={tiles}
            onChange={(status) => setFilters({ ...filters, status })}
          />
          <PeriodFilter
            value={{ from: filters.from, to: filters.to }}
            onChange={(span) => setFilters({ ...filters, ...span })}
            today={today}
          />
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
              {t("Sort order")}
              <select
                value={filters.sort}
                onChange={(event) => setFilters({ ...filters, sort: event.target.value as PurchaseOrderFilters["sort"] })}
              >
                {Object.entries(ORDER_LABELS).map(([key, label]) => (
                  <option key={key} value={key}>
                    {t(label)}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <Notice message={error ?? targetsError} />
          {selected.length > 0 && (
            <SelectionBar
              selected={selected}
              busy={acting}
              onAct={(action) => void actOnSelected(action)}
              onClear={() => setSelectedIds([])}
            />
          )}
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
              <BoxIcon size={40} aria-hidden />
              <p role="status">{t("No purchases found.")}</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_PURCHASE_FILTERS })}>
                {t("Clear filters")}
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <BoxIcon size={40} aria-hidden />
              <p>{t("No purchases yet.")}</p>
              <p className="muted">{t("Create an order here, approve it and record what the supplier delivers.")}</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    {canWrite && (
                      <th>
                        <input
                          type="checkbox"
                          aria-label={t("Select all purchases in the list")}
                          checked={items.length > 0 && selected.length === items.length}
                          onChange={(event) => setSelectedIds(event.target.checked ? items.map((order) => order.id) : [])}
                        />
                      </th>
                    )}
                    <th>{t("No.")}</th>
                    <th>{t("Supplier")}</th>
                    {columns.isShown("ordered") && <th>{t("Date")}</th>}
                    {columns.isShown("expected") && <th>{t("Expected")}</th>}
                    {columns.isShown("invoice") && <th>{t("Invoice no.")}</th>}
                    {columns.isShown("total") && <th className="num">{t("Total")}</th>}
                    <th>{t("Status")}</th>
                    <th>
                      <span className="visually-hidden">{t("Actions")}</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((order, index) => (
                    <Fragment key={order.id}>
                      <ListRow index={index} highlighted={order.id === highlightedId}>
                        {canWrite && (
                          <td>
                            <input
                              type="checkbox"
                              aria-label={t("Select purchase {number}", { number: order.number })}
                              checked={selectedIds.includes(order.id)}
                              onChange={(event) =>
                                setSelectedIds(event.target.checked ? [...selectedIds, order.id] : selectedIds.filter((id) => id !== order.id))
                              }
                            />
                          </td>
                        )}
                        <td>{order.number}</td>
                        <td>
                          <div className="cell-title">{names.get(order.supplier_id) ?? ""}</div>
                          <div className="muted">
                            {order.lines.length === 1
                              ? t("{count} item", { count: order.lines.length })
                              : t("{count} items", { count: order.lines.length })}
                          </div>
                        </td>
                        {columns.isShown("ordered") && <td>{formatDate(order.ordered_on)}</td>}
                        {columns.isShown("expected") && <td>{order.expected_on === null ? "—" : formatDate(order.expected_on)}</td>}
                        {columns.isShown("invoice") && <td>{invoiceNumber(order.invoice_key) || "—"}</td>}
                        {columns.isShown("total") && <td className="num">{formatMoney(order.total)}</td>}
                        <td>
                          <span className={purchaseStatusClass(order.status)}>{t(PURCHASE_STATUS_LABELS[order.status])}</span>
                        </td>
                        <td className="row-actions">
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            icon={FileTextIcon}
                            aria-label={t("Details of purchase {number}", { number: order.number })}
                            aria-expanded={isOpen("details", order.id)}
                            onClick={() => toggle("details", order.id)}
                          >
                            {t("Details")}
                          </Button>
                          {canWrite && order.status === "draft" && (
                            <>
                              <Button
                                iconOnly
                                icon={SquarePenIcon}
                                type="button"
                                variant="ghost"
                                aria-label={t("Change purchase {number}", { number: order.number })}
                                disabled={saving || acting}
                                onClick={() => {
                                  setEditing(order);
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
                                aria-label={t("Approve purchase {number}", { number: order.number })}
                                aria-expanded={isOpen("approve", order.id)}
                                onClick={() => toggle("approve", order.id)}
                              >
                                {t("Approve")}
                              </Button>
                            </>
                          )}
                          {canWrite && (order.status === "approved" || order.status === "partially_received") && (
                            <Button
                              iconOnly
                              icon={BoxIcon}
                              type="button"
                              variant="ghost"
                              aria-label={t("Receive purchase {number}", { number: order.number })}
                              aria-expanded={isOpen("receive", order.id)}
                              onClick={() => toggle("receive", order.id)}
                            >
                              {t("Receive")}
                            </Button>
                          )}
                          {canWrite && order.status === "partially_received" && (
                            <Button
                              type="button"
                              variant="ghost"
                              className="btn-small"
                              aria-label={t("Close purchase {number}", { number: order.number })}
                              aria-expanded={isOpen("close", order.id)}
                              onClick={() => toggle("close", order.id)}
                            >
                              {t("Close order")}
                            </Button>
                          )}
                          {canWrite && (order.status === "draft" || order.status === "approved") && (
                            <Button
                              type="button"
                              variant="ghost"
                              className="btn-small"
                              aria-label={t("Cancel purchase {number}", { number: order.number })}
                              aria-expanded={isOpen("cancel", order.id)}
                              onClick={() => toggle("cancel", order.id)}
                            >
                              {t("Cancel")}
                            </Button>
                          )}
                          {canWrite && (order.status === "approved" || order.status === "partially_received") && (
                            <Button
                              iconOnly
                              type="button"
                              variant="ghost"
                              icon={SendIcon}
                              aria-label={t("Send purchase {number} to the supplier", { number: order.number })}
                              aria-expanded={isOpen("send", order.id)}
                              onClick={() => toggle("send", order.id)}
                            >
                              {t("Send to supplier")}
                            </Button>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={t("History of purchase {number}", { number: order.number })}
                            aria-expanded={isOpen("history", order.id)}
                            icon={HistoryIcon}
                            onClick={() => toggle("history", order.id)}
                          >
                            {t("History")}
                          </Button>
                        </td>
                      </ListRow>
                      {panel?.id === order.id && panel.kind === "details" && (
                        <tr>
                          <td colSpan={span} role="region" aria-label={t("Details of purchase {number}", { number: order.number })}>
                            <DetailsPanel companyId={companyId} order={order} names={names} canWrite={canWrite} onBusyChange={setActing} />
                          </td>
                        </tr>
                      )}
                      {panel?.id === order.id && panel.kind === "receive" && (
                        <tr>
                          <td colSpan={span} role="region" aria-label={t("Receipt of purchase {number}", { number: order.number })}>
                            <ReceivePanel companyId={companyId} order={order} onBusyChange={setActing} onReceived={(receipt) => void received(order, receipt)} />
                          </td>
                        </tr>
                      )}
                      {panel?.id === order.id && (panel.kind === "approve" || panel.kind === "cancel" || panel.kind === "close") && (
                        <tr>
                          <td colSpan={span} role="region" aria-label={t("Confirmation of purchase {number}", { number: order.number })}>
                            <div className="settlements">
                              <p>{CONFIRMATIONS[panel.kind].text(t, order)}</p>
                              <Notice message={actionError} />
                              <div className="form-actions">
                                <Button type="button" pending={acting} onClick={() => void act(order, panel.kind as Action)}>
                                  {t(CONFIRMATIONS[panel.kind].button)}
                                </Button>
                                <Button type="button" variant="ghost" disabled={acting} onClick={() => setPanel(null)}>
                                  {t("Back")}
                                </Button>
                              </div>
                            </div>
                          </td>
                        </tr>
                      )}
                      {panel?.id === order.id && panel.kind === "send" && (
                        <tr>
                          <td colSpan={span} role="region" aria-label={t("Sending of purchase {number}", { number: order.number })}>
                            <MailPanel
                              what={t("the order")}
                              send={(to) => api.sendPurchaseOrder(companyId, order.id, to)}
                              loadHistory={() => api.purchaseOrderMails(companyId, order.id).then((reply) => reply.items)}
                              onClose={() => setPanel(null)}
                            />
                          </td>
                        </tr>
                      )}
                      {panel?.id === order.id && panel.kind === "history" && (
                        <tr>
                          <td colSpan={span} role="region" aria-label={t("History of purchase {number}", { number: order.number })}>
                            <History
                              load={historyLoader(order.id)}
                              fieldLabel={(field) => fieldLabel(t, field)}
                              valueLabel={(field, value) => displayValue(t, field, value, names)}
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
          <Sheet open={editing !== null} locked={saving} title={t("Change purchase {number}", { number: shown?.number ?? "" })} onClose={closeSheet}>
            {shown !== null &&
              (targetsLoaded ? (
                <PurchaseForm
                  key={shown.id}
                  companyId={companyId}
                  order={shown}
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

/** What the receipts of an order, read from the API; `null` while they are on their way. */
function useReceipts(companyId: string, orderId: string) {
  const [receipts, setReceipts] = useState<PurchaseReceipt[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  useEffect(() => {
    let current = true;
    api
      .listReceipts(companyId, orderId)
      .then((reply) => current && setReceipts(reply.items))
      .catch((failure: unknown) => current && setLoadError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId, orderId]);
  return { receipts, setReceipts, loadError };
}

/** The order's data, what each line has received and what is left, and its receipts with their payables. */
function DetailsPanel({
  companyId,
  order,
  names,
  canWrite,
  onBusyChange,
}: {
  companyId: string;
  order: PurchaseOrder;
  names: Map<string, string>;
  canWrite: boolean;
  /** Tells the page a change to the order's receipts is on its way: the order must not be edited meanwhile. */
  onBusyChange: (busy: boolean) => void;
}) {
  const { t, rich } = useI18n();
  const { receipts, setReceipts, loadError } = useReceipts(companyId, order.id);
  const [finishing, setFinishing] = useState<string | null>(null);
  const [finishError, setFinishError] = useState<string | null>(null);
  useEffect(() => {
    onBusyChange(finishing !== null);
    return () => onBusyChange(false);
  }, [finishing, onBusyChange]);
  const received = receipts === null ? null : receivedByLine(order.lines, receipts);
  const left = receipts === null ? null : leftByLine(order.lines, receipts);

  async function finish(receipt: PurchaseReceipt) {
    if (finishing !== null) {
      return;
    }
    setFinishing(receipt.id);
    setFinishError(null);
    try {
      const done = await api.finishReceipt(companyId, order.id, receipt.id);
      setReceipts((current) => current?.map((item) => (item.id === done.id ? done : item)) ?? null);
    } catch (failure) {
      setFinishError(errorMessage(failure));
    } finally {
      setFinishing(null);
    }
  }

  return (
    <div className="settlements">
      <p>
        {rich("{supplier} · ordered on {date}", {
          supplier: <strong>{names.get(order.supplier_id) ?? ""}</strong>,
          date: formatDate(order.ordered_on),
        })}
        {order.expected_on !== null && ` · ${t("expected for {date}", { date: formatDate(order.expected_on) })}`}
      </p>
      <p className="muted">
        {order.payment_term_id === null
          ? t("Cash, on the day of receipt")
          : t("Term {name}", { name: names.get(order.payment_term_id) ?? "" })}
        {` · ${names.get(order.category_id) ?? ""}`}
        {order.cost_center_id !== null && ` · ${names.get(order.cost_center_id) ?? ""}`}
        {order.notes !== null && ` · ${order.notes}`}
      </p>
      {order.invoice_key !== null && <p className="muted tabular">{t("NF-e (key): {key}", { key: order.invoice_key })}</p>}
      <Notice message={loadError} />
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>{t("Item")}</th>
              <th className="num">{t("Ordered")}</th>
              <th className="num">{t("Received")}</th>
              <th className="num">{t("Remaining")}</th>
              <th className="num">{t("Cost")}</th>
              <th className="num">{t("Value")}</th>
            </tr>
          </thead>
          <tbody>
            {order.lines.map((line, index) => (
              <tr key={index}>
                <td>{line.description}</td>
                <td className="num">{formatDecimal(line.quantity)}</td>
                <td className="num">{received === null ? "…" : formatDecimal(quantityToApi(received[index]))}</td>
                <td className="num">{left === null ? "…" : formatDecimal(left[index])}</td>
                <td className="num">{formatMoney(line.unit_price)}</td>
                <td className="num">{formatMoney(line.total)}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr>
              <th colSpan={5}>{t("Total")}</th>
              <td className="num">{formatMoney(order.total)}</td>
            </tr>
          </tfoot>
        </table>
      </div>
      <h3>{t("Receipts")}</h3>
      <Notice message={finishError} />
      {receipts === null ? (
        loadError === null && (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        )
      ) : receipts.length === 0 ? (
        <p className="muted">{t("Nothing has been received on this purchase.")}</p>
      ) : (
        <ul className="history">
          {receipts.map((receipt) => (
            <li key={receipt.id}>
              {formatDate(receipt.received_on)} ·{" "}
              {receipt.items
                .map((item) => `${order.lines[item.line - 1]?.description ?? t("item")} × ${formatDecimal(item.quantity)}`)
                .join(", ")}{" "}
              · {formatMoney(receipt.total)}
              {receipt.payable_id !== null ? (
                <>
                  {" · "}
                  <Link to={`/companies/${companyId}/entries`}>{t("Payable in the entries")}</Link>
                </>
              ) : receipt.total === "0" ? (
                ` · ${t("nothing to pay")}`
              ) : (
                <>
                  {` · ${t("payable not entered yet")} `}
                  {canWrite && (
                    <Button
                      type="button"
                      variant="ghost"
                      className="btn-small"
                      aria-label={t("Finish the receipt of {date}", { date: formatDate(receipt.received_on) })}
                      pending={finishing === receipt.id}
                      onClick={() => void finish(receipt)}
                    >
                      {t("Finish receipt")}
                    </Button>
                  )}
                </>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

const isZero = (typed: string) => /^0+([.,]0*)?$/.test(typed.trim());

/** The form of a receipt: a quantity per line, each starting at what is still to arrive. */
function ReceivePanel({
  companyId,
  order,
  onBusyChange,
  onReceived,
}: {
  companyId: string;
  order: PurchaseOrder;
  /** Tells the page a receipt is being saved: the order must not be edited meanwhile. */
  onBusyChange: (busy: boolean) => void;
  onReceived: (receipt: PurchaseReceipt) => void;
}) {
  const t = useT();
  const { receipts, loadError } = useReceipts(companyId, order.id);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    onBusyChange(saving);
    return () => onBusyChange(false);
  }, [saving, onBusyChange]);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    const items: { line: number; quantity: string }[] = [];
    for (const [index, line] of order.lines.entries()) {
      const typed = field(`line-${index + 1}`);
      if (typed === "" || isZero(typed)) continue;
      const quantity = toQuantity(typed);
      if (quantity === null) {
        setError(t("The quantity of {item} must be a number with up to 3 decimals.", { item: line.description }));
        return;
      }
      items.push({ line: index + 1, quantity: quantityToApi(quantity) });
    }
    if (items.length === 0) {
      setError(t("Enter the quantity received of at least one item."));
      return;
    }
    if (!isIsoDate(field("received_on"))) {
      setError(t("Enter the receipt date."));
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const receipt = await api.receivePurchaseOrder(companyId, order.id, {
        received_on: field("received_on"),
        notes: field("notes") || undefined,
        items,
      });
      onReceived(receipt);
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  if (receipts === null) {
    return loadError === null ? (
      <p role="status" className="status">
        <Spinner /> {t("Loading…")}
      </p>
    ) : (
      <Notice message={loadError} />
    );
  }
  const left = leftByLine(order.lines, receipts);
  return (
    <form className="settle-form" onSubmit={(event) => void submit(event)}>
      <h3>{t("Record receipt")}</h3>
      {order.lines.map((line, index) => (
        <label key={index}>
          {t("{item} (remaining: {left})", { item: line.description, left: formatDecimal(left[index]) })}
          <input
            name={`line-${index + 1}`}
            aria-label={t("Quantity received of {item}", { item: line.description })}
            inputMode="decimal"
            autoComplete="off"
            placeholder="0"
            defaultValue={toInputDecimal(left[index])}
          />
        </label>
      ))}
      <label>
        {t("Receipt date")}
        <input name="received_on" type="date" required defaultValue={todayIso()} />
      </label>
      <label>
        {t("Notes")}
        <input name="notes" maxLength={200} autoComplete="off" />
      </label>
      <Notice message={error} />
      <Button type="submit" icon={PlusIcon} pending={saving}>
        {t("Record receipt")}
      </Button>
    </form>
  );
}

function fieldLabel(t: Translate, field: string): string {
  const label = FIELD_LABELS[field];
  return label === undefined ? field : t(label);
}

/** A value of the trail as a person reads it: names for references, words for keys. */
function displayValue(t: Translate, field: string, value: string | null, names: Map<string, string>): string {
  if (value === null) return "—";
  if (["supplier_id", "payment_term_id", "category_id", "cost_center_id"].includes(field)) {
    return names.get(value) ?? value;
  }
  if (field === "status") {
    const label = PURCHASE_STATUS_LABELS[value as PurchaseStatus];
    return label === undefined ? value : t(label);
  }
  if (field === "ordered_on" || field === "expected_on") return formatDate(value);
  if (field === "lines") return linesTrailText(value, names);
  return value;
}

/** What the ticked orders can be given in one go: how many each action reaches, and a confirmation before it is made. */
function SelectionBar({
  selected,
  busy,
  onAct,
  onClear,
}: {
  selected: PurchaseOrder[];
  busy: boolean;
  onAct: (action: "approve" | "cancel") => void;
  onClear: () => void;
}) {
  const t = useT();
  const [asking, setAsking] = useState<"approve" | "cancel" | null>(null);
  const reach = (action: "approve" | "cancel") => selected.filter((order) => BULK_ELIGIBLE[action].includes(order.status)).length;
  const plural = reach(asking ?? "approve") !== 1;
  const willDo =
    asking === "cancel"
      ? plural
        ? t("It will cancel {count} purchases", { count: reach("cancel") })
        : t("It will cancel {count} purchase", { count: reach("cancel") })
      : plural
        ? t("It will approve {count} purchases", { count: reach("approve") })
        : t("It will approve {count} purchase", { count: reach("approve") });

  return (
    <div className="selection-bar" role="region" aria-label={t("Selected purchases")}>
      <span>
        {selected.length === 1
          ? t("{count} purchase selected", { count: selected.length })
          : t("{count} purchases selected", { count: selected.length })}
      </span>
      {asking === null ? (
        <>
          {(["approve", "cancel"] as const).map((action) => (
            <Button key={action} type="button" variant="ghost" disabled={busy || reach(action) === 0} onClick={() => setAsking(action)}>
              {action === "approve" ? t("Approve ({count})", { count: reach(action) }) : t("Cancel ({count})", { count: reach(action) })}
            </Button>
          ))}
          <Button type="button" variant="ghost" disabled={busy} onClick={onClear}>
            {t("Clear selection")}
          </Button>
        </>
      ) : (
        <>
          <span>
            {willDo}
            {reach(asking) < selected.length && t("; the others do not apply and stay as they are")}.{" "}
            {asking === "cancel" ? t("A cancelled order is not reopened.") : t("Once approved, the items do not change.")}
          </span>
          <Button
            type="button"
            pending={busy}
            onClick={() => {
              onAct(asking);
              setAsking(null);
            }}
          >
            {t("Confirm")}
          </Button>
          <Button type="button" variant="ghost" disabled={busy} onClick={() => setAsking(null)}>
            {t("Back")}
          </Button>
        </>
      )}
    </div>
  );
}
