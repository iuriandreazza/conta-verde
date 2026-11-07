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
import { formatDecimal, formatMoney, toInputDecimal } from "../lib/decimal";
import { invoiceNumber } from "../lib/invoice";
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
  supplier_id: "Fornecedor",
  ordered_on: "Data do pedido",
  expected_on: "Previsão de entrega",
  payment_term_id: "Prazo de pagamento",
  category_id: "Categoria",
  cost_center_id: "Centro de custo",
  notes: "Observações",
  lines: "Itens",
  status: "Situação",
};

type PanelKind = "details" | "receive" | "approve" | "cancel" | "close" | "history" | "send";
type Panel = { kind: PanelKind; id: string };
type Action = "approve" | "cancel" | "close";

const ORDER_LABELS: Record<PurchaseOrderFilters["sort"], string> = {
  "-number": "Mais recentes primeiro",
  number: "Mais antigas primeiro",
};

const CONFIRMATIONS: Record<Action, { text: (order: PurchaseOrder) => string; button: string; done: (order: PurchaseOrder) => string }> = {
  approve: {
    text: (order) =>
      `Aprovar combina o pedido de ${formatMoney(order.total)} com o fornecedor: depois disso os itens não mudam.`,
    button: "Aprovar compra",
    done: (order) => `Compra nº ${order.number} aprovada.`,
  },
  cancel: {
    text: () => "Cancelar o pedido o encerra: ele não é reaberto, e só vale antes de qualquer recebimento.",
    button: "Cancelar a compra",
    done: (order) => `Compra nº ${order.number} cancelada.`,
  },
  close: {
    text: () => "Encerrar dá o pedido por recebido: o que ainda não chegou é dispensado e não será cobrado.",
    button: "Encerrar a compra",
    done: (order) => `Compra nº ${order.number} encerrada: o que não chegou foi dispensado.`,
  },
};

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
const PURCHASE_COLUMNS = ["Número", "Fornecedor", "Data", "Previsão", "Situação", "Total"];

export function PurchasesPage() {
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
  const [actionError, setActionError] = useState<string | null>(null);
  // What an order points at is read once: the form needs it, and so do the names in the list.
  const targets = useSalesTargets(companyId);
  const { names, error: targetsError, loaded: targetsLoaded } = targets;

  const { items, setItems, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<PurchaseOrder, PurchaseOrderFilters>(
      (applied, after) => api.listPurchaseOrders(companyId, { ...applied, from: wholeDate(applied.from), to: wholeDate(applied.to) }, after),
      filters,
    );
  const today = todayIso();
  const isFiltered = applied.q !== "" || applied.status !== "" || applied.from !== "" || applied.to !== "";
  const counts = useCounts(() => api.getPurchaseStatusCounts(companyId, { q: applied.q, from: wholeDate(applied.from), to: wholeDate(applied.to) }), items, !loading);
  const tiles: TileOption<PurchaseOrderFilters["status"]>[] = [
    { key: "", label: "Todas", count: counts === null ? null : Object.values(counts).reduce((sum, n) => sum + n, 0) },
    ...(Object.entries(PURCHASE_STATUS_LABELS) as [PurchaseStatus, string][]).map(([key, label]) => ({
      key,
      label,
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
      setSaved(`Compra nº ${updated.number} atualizada.`);
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
      setSaved(CONFIRMATIONS[action].done(updated));
    } catch (failure) {
      setActionError(errorMessage(failure));
    } finally {
      setActing(false);
    }
  }

  /** A receipt changes the order's status, which only the order's own answer tells. */
  async function received(order: PurchaseOrder, receipt: PurchaseReceipt) {
    setPanel(null);
    setSaved(
      receipt.payable_id === null && receipt.total !== "0"
        ? "Recebimento registrado, mas a conta a pagar ainda não foi lançada: conclua o recebimento nos detalhes da compra."
        : `Recebimento registrado${receipt.payable_id === null ? "" : `: a conta a pagar de ${formatMoney(receipt.total)} está nos lançamentos`}.`,
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
        title="Compras"
        description="Pedidos aos fornecedores: cada recebimento, mesmo parcial, gera a conta a pagar do que chegou pelo prazo de pagamento."
      />
      <div className="list-actions">
        {canWrite && (
          <MenuButton
            label="Novo"
            icon={PlusIcon}
            variant="primary"
            items={[
              { key: "purchase", label: "Nova compra", icon: PlusIcon, onSelect: () => openCreate("purchase") },
              { key: "invoice", label: "Entrar NF-e (XML)", icon: FileTextIcon, onSelect: () => openCreate("purchaseImport") },
            ]}
          />
        )}
        <ListTools
          filename="compras"
          header={PURCHASE_COLUMNS}
          fetchPage={(after) => api.listPurchaseOrders(companyId, { ...applied, from: wholeDate(applied.from), to: wholeDate(applied.to) }, after)}
          toRow={(order) => [
            String(order.number),
            names.get(order.supplier_id) ?? "",
            order.ordered_on,
            order.expected_on ?? "",
            PURCHASE_STATUS_LABELS[order.status],
            toInputDecimal(order.total),
          ]}
        />
      </div>
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
          <h2>Pedidos de compra</h2>
          <FilterTiles
            label="Situação"
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
              Buscar
              <input
                type="search"
                value={filters.q}
                placeholder="Número, item ou observação"
                autoComplete="off"
                maxLength={100}
                onChange={(event) => setFilters({ ...filters, q: event.target.value })}
              />
            </label>
            <label>
              Ordem
              <select
                value={filters.sort}
                onChange={(event) => setFilters({ ...filters, sort: event.target.value as PurchaseOrderFilters["sort"] })}
              >
                {Object.entries(ORDER_LABELS).map(([key, label]) => (
                  <option key={key} value={key}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <Notice message={error ?? targetsError} />
          {loading ? (
            <p role="status" className="status">
              <Spinner /> Carregando…
            </p>
          ) : items.length === 0 && error ? (
            <Button type="button" variant="ghost" onClick={() => void reload()}>
              Tentar de novo
            </Button>
          ) : items.length === 0 && isFiltered ? (
            <div className="empty">
              <BoxIcon size={40} aria-hidden />
              <p role="status">Nenhuma compra encontrada.</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_PURCHASE_FILTERS })}>
                Limpar filtros
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <BoxIcon size={40} aria-hidden />
              <p>Nenhuma compra ainda.</p>
              <p className="muted">Crie um pedido aqui, aprove e registre o que o fornecedor entregar.</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Nº</th>
                    <th>Fornecedor</th>
                    <th>Data</th>
                    <th>Previsão</th>
                    <th>Nº da NF</th>
                    <th className="num">Total</th>
                    <th>Situação</th>
                    <th>
                      <span className="visually-hidden">Ações</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((order, index) => (
                    <Fragment key={order.id}>
                      <ListRow index={index} highlighted={order.id === highlightedId}>
                        <td>{order.number}</td>
                        <td>
                          <div className="cell-title">{names.get(order.supplier_id) ?? ""}</div>
                          <div className="muted">
                            {order.lines.length} {order.lines.length === 1 ? "item" : "itens"}
                          </div>
                        </td>
                        <td>{formatDate(order.ordered_on)}</td>
                        <td>{order.expected_on === null ? "—" : formatDate(order.expected_on)}</td>
                        <td>{invoiceNumber(order.invoice_key) || "—"}</td>
                        <td className="num">{formatMoney(order.total)}</td>
                        <td>
                          <span className={purchaseStatusClass(order.status)}>{PURCHASE_STATUS_LABELS[order.status]}</span>
                        </td>
                        <td className="row-actions">
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            icon={FileTextIcon}
                            aria-label={`Detalhes da compra ${order.number}`}
                            aria-expanded={isOpen("details", order.id)}
                            onClick={() => toggle("details", order.id)}
                          >
                            Detalhes
                          </Button>
                          {canWrite && order.status === "draft" && (
                            <>
                              <Button
                                iconOnly
                                icon={SquarePenIcon}
                                type="button"
                                variant="ghost"
                                aria-label={`Alterar a compra ${order.number}`}
                                disabled={saving || acting}
                                onClick={() => {
                                  setEditing(order);
                                  setFormError(null);
                                  setSaved(null);
                                }}
                              >
                                Alterar
                              </Button>
                              <Button
                                iconOnly
                                icon={CheckIcon}
                                type="button"
                                variant="ghost"
                                aria-label={`Aprovar a compra ${order.number}`}
                                aria-expanded={isOpen("approve", order.id)}
                                onClick={() => toggle("approve", order.id)}
                              >
                                Aprovar
                              </Button>
                            </>
                          )}
                          {canWrite && (order.status === "approved" || order.status === "partially_received") && (
                            <Button
                              iconOnly
                              icon={BoxIcon}
                              type="button"
                              variant="ghost"
                              aria-label={`Receber a compra ${order.number}`}
                              aria-expanded={isOpen("receive", order.id)}
                              onClick={() => toggle("receive", order.id)}
                            >
                              Receber
                            </Button>
                          )}
                          {canWrite && order.status === "partially_received" && (
                            <Button
                              type="button"
                              variant="ghost"
                              className="btn-small"
                              aria-label={`Encerrar a compra ${order.number}`}
                              aria-expanded={isOpen("close", order.id)}
                              onClick={() => toggle("close", order.id)}
                            >
                              Encerrar
                            </Button>
                          )}
                          {canWrite && (order.status === "draft" || order.status === "approved") && (
                            <Button
                              type="button"
                              variant="ghost"
                              className="btn-small"
                              aria-label={`Cancelar a compra ${order.number}`}
                              aria-expanded={isOpen("cancel", order.id)}
                              onClick={() => toggle("cancel", order.id)}
                            >
                              Cancelar
                            </Button>
                          )}
                          {canWrite && (order.status === "approved" || order.status === "partially_received") && (
                            <Button
                              iconOnly
                              type="button"
                              variant="ghost"
                              icon={SendIcon}
                              aria-label={`Enviar a compra ${order.number} ao fornecedor`}
                              aria-expanded={isOpen("send", order.id)}
                              onClick={() => toggle("send", order.id)}
                            >
                              Enviar ao fornecedor
                            </Button>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={`Histórico da compra ${order.number}`}
                            aria-expanded={isOpen("history", order.id)}
                            icon={HistoryIcon}
                            onClick={() => toggle("history", order.id)}
                          >
                            Histórico
                          </Button>
                        </td>
                      </ListRow>
                      {panel?.id === order.id && panel.kind === "details" && (
                        <tr>
                          <td colSpan={8} role="region" aria-label={`Detalhes da compra ${order.number}`}>
                            <DetailsPanel companyId={companyId} order={order} names={names} canWrite={canWrite} onBusyChange={setActing} />
                          </td>
                        </tr>
                      )}
                      {panel?.id === order.id && panel.kind === "receive" && (
                        <tr>
                          <td colSpan={8} role="region" aria-label={`Recebimento da compra ${order.number}`}>
                            <ReceivePanel companyId={companyId} order={order} onBusyChange={setActing} onReceived={(receipt) => void received(order, receipt)} />
                          </td>
                        </tr>
                      )}
                      {panel?.id === order.id && (panel.kind === "approve" || panel.kind === "cancel" || panel.kind === "close") && (
                        <tr>
                          <td colSpan={8} role="region" aria-label={`Confirmação da compra ${order.number}`}>
                            <div className="settlements">
                              <p>{CONFIRMATIONS[panel.kind].text(order)}</p>
                              <Notice message={actionError} />
                              <div className="form-actions">
                                <Button type="button" pending={acting} onClick={() => void act(order, panel.kind as Action)}>
                                  {CONFIRMATIONS[panel.kind].button}
                                </Button>
                                <Button type="button" variant="ghost" disabled={acting} onClick={() => setPanel(null)}>
                                  Voltar
                                </Button>
                              </div>
                            </div>
                          </td>
                        </tr>
                      )}
                      {panel?.id === order.id && panel.kind === "send" && (
                        <tr>
                          <td colSpan={8} role="region" aria-label={`Envio da compra ${order.number}`}>
                            <MailPanel
                              what="o pedido"
                              send={(to) => api.sendPurchaseOrder(companyId, order.id, to)}
                              onClose={() => setPanel(null)}
                            />
                          </td>
                        </tr>
                      )}
                      {panel?.id === order.id && panel.kind === "history" && (
                        <tr>
                          <td colSpan={8} role="region" aria-label={`Histórico da compra ${order.number}`}>
                            <History
                              load={historyLoader(order.id)}
                              fieldLabel={(field) => FIELD_LABELS[field] ?? field}
                              valueLabel={(field, value) => displayValue(field, value, names)}
                              emptyMessage="Sem registros."
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
              Carregar mais
            </Button>
          )}
          {panel === null && <Notice message={actionError} />}
        </section>
        {canWrite && (
          <Sheet open={editing !== null} locked={saving} title={`Alterar a compra ${shown?.number ?? ""}`} onClose={closeSheet}>
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
                  <Spinner /> Carregando…
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
        <strong>{names.get(order.supplier_id) ?? ""}</strong> · pedido em {formatDate(order.ordered_on)}
        {order.expected_on !== null && ` · previsto para ${formatDate(order.expected_on)}`}
      </p>
      <p className="muted">
        {order.payment_term_id === null ? "À vista, no dia do recebimento" : `Prazo ${names.get(order.payment_term_id) ?? ""}`}
        {` · ${names.get(order.category_id) ?? ""}`}
        {order.cost_center_id !== null && ` · ${names.get(order.cost_center_id) ?? ""}`}
        {order.notes !== null && ` · ${order.notes}`}
      </p>
      {order.invoice_key !== null && <p className="muted tabular">NF-e (chave): {order.invoice_key}</p>}
      <Notice message={loadError} />
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Item</th>
              <th className="num">Pedido</th>
              <th className="num">Recebido</th>
              <th className="num">Falta</th>
              <th className="num">Custo</th>
              <th className="num">Valor</th>
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
              <th colSpan={5}>Total</th>
              <td className="num">{formatMoney(order.total)}</td>
            </tr>
          </tfoot>
        </table>
      </div>
      <h3>Recebimentos</h3>
      <Notice message={finishError} />
      {receipts === null ? (
        loadError === null && (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        )
      ) : receipts.length === 0 ? (
        <p className="muted">Nada foi recebido desta compra.</p>
      ) : (
        <ul className="history">
          {receipts.map((receipt) => (
            <li key={receipt.id}>
              {formatDate(receipt.received_on)} ·{" "}
              {receipt.items
                .map((item) => `${order.lines[item.line - 1]?.description ?? "item"} × ${formatDecimal(item.quantity)}`)
                .join(", ")}{" "}
              · {formatMoney(receipt.total)}
              {receipt.payable_id !== null ? (
                <>
                  {" · "}
                  <Link to={`/companies/${companyId}/entries`}>Conta a pagar nos lançamentos</Link>
                </>
              ) : receipt.total === "0" ? (
                " · sem valor a pagar"
              ) : (
                <>
                  {" · conta a pagar ainda não lançada "}
                  {canWrite && (
                    <Button
                      type="button"
                      variant="ghost"
                      className="btn-small"
                      aria-label={`Concluir o recebimento de ${formatDate(receipt.received_on)}`}
                      pending={finishing === receipt.id}
                      onClick={() => void finish(receipt)}
                    >
                      Concluir recebimento
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
        setError(`A quantidade de ${line.description} precisa ser um número com até 3 casas decimais.`);
        return;
      }
      items.push({ line: index + 1, quantity: quantityToApi(quantity) });
    }
    if (items.length === 0) {
      setError("Informe a quantidade recebida de pelo menos um item.");
      return;
    }
    if (!isIsoDate(field("received_on"))) {
      setError("Informe a data do recebimento.");
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
        <Spinner /> Carregando…
      </p>
    ) : (
      <Notice message={loadError} />
    );
  }
  const left = leftByLine(order.lines, receipts);
  return (
    <form className="settle-form" onSubmit={(event) => void submit(event)}>
      <h3>Registrar recebimento</h3>
      {order.lines.map((line, index) => (
        <label key={index}>
          {line.description} (falta: {formatDecimal(left[index])})
          <input
            name={`line-${index + 1}`}
            aria-label={`Quantidade recebida de ${line.description}`}
            inputMode="decimal"
            autoComplete="off"
            placeholder="0"
            defaultValue={toInputDecimal(left[index])}
          />
        </label>
      ))}
      <label>
        Data do recebimento
        <input name="received_on" type="date" required defaultValue={todayIso()} />
      </label>
      <label>
        Observações
        <input name="notes" maxLength={200} autoComplete="off" />
      </label>
      <Notice message={error} />
      <Button type="submit" icon={PlusIcon} pending={saving}>
        Registrar recebimento
      </Button>
    </form>
  );
}

/** A value of the trail as a person reads it: names for references, words for keys. */
function displayValue(field: string, value: string | null, names: Map<string, string>): string {
  if (value === null) return "—";
  if (["supplier_id", "payment_term_id", "category_id", "cost_center_id"].includes(field)) {
    return names.get(value) ?? value;
  }
  if (field === "status") return PURCHASE_STATUS_LABELS[value as PurchaseStatus] ?? value;
  if (field === "ordered_on" || field === "expected_on") return formatDate(value);
  if (field === "lines") return linesTrailText(value, names);
  return value;
}
