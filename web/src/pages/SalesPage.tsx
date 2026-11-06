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
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

const FIELD_LABELS: Record<string, string> = {
  customer_id: "Cliente",
  seller_id: "Vendedor",
  sold_on: "Data da venda",
  payment_term_id: "Prazo de pagamento",
  category_id: "Categoria",
  cost_center_id: "Centro de custo",
  notes: "Observações",
  lines: "Itens",
  status: "Situação",
  quote_id: "Orçamento",
};

type Panel = { kind: "complete" | "returns" | "history" | "cancel"; id: string };

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function SalesPage() {
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
      setSaved(`Pedido nº ${updated.number} atualizado.`);
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
          ? `Venda nº ${updated.number} concluída: a conta a receber está nos lançamentos.`
          : `Venda nº ${updated.number} cancelada${sale.status === "completed" ? " e a conta a receber também" : ""}.`,
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
        title="Vendas"
        description="Pedidos e vendas: concluir gera a conta a receber pelo prazo de pagamento; devoluções geram a conta a pagar do reembolso."
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("sale")}>
            Novo pedido
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
          <h2>Pedidos e vendas</h2>
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
              Situação
              <select
                value={filters.status}
                onChange={(event) => setFilters({ ...filters, status: event.target.value as SaleFilters["status"] })}
              >
                <option value="">Todas</option>
                {Object.entries(SALE_STATUS_LABELS).map(([key, label]) => (
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
              <HandCoinsIcon size={40} aria-hidden />
              <p role="status">Nenhuma venda encontrada.</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_SALE_FILTERS })}>
                Limpar filtros
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <HandCoinsIcon size={40} aria-hidden />
              <p>Nenhuma venda ainda.</p>
              <p className="muted">Crie um pedido aqui, ou converta um orçamento aprovado.</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Nº</th>
                    <th>Cliente</th>
                    <th>Data</th>
                    <th className="num">Total</th>
                    <th>Situação</th>
                    <th>
                      <span className="visually-hidden">Ações</span>
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
                            {sale.lines.length} {sale.lines.length === 1 ? "item" : "itens"}
                            {sale.seller_id !== null && ` · ${names.get(sale.seller_id) ?? ""}`}
                            {sale.contract_id !== null && " · De um contrato"}
                          </div>
                        </td>
                        <td>{formatDate(sale.sold_on)}</td>
                        <td className="num">{formatMoney(sale.total)}</td>
                        <td>
                          <span className={statusClass(sale.status)}>{SALE_STATUS_LABELS[sale.status]}</span>
                        </td>
                        <td className="row-actions">
                          {canWrite && sale.status === "open" && (
                            <>
                              <Button
                                iconOnly
                                icon={SquarePenIcon}
                                type="button"
                                variant="ghost"
                                aria-label={`Alterar o pedido ${sale.number}`}
                                disabled={saving}
                                onClick={() => {
                                  setEditing(sale);
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
                                aria-label={`Concluir a venda ${sale.number}`}
                                aria-expanded={panel?.kind === "complete" && panel.id === sale.id}
                                onClick={() => toggle("complete", sale.id)}
                              >
                                Concluir
                              </Button>
                            </>
                          )}
                          {canWrite && sale.status === "completed" && (
                            <Button
                              iconOnly
                              icon={RotateCCWIcon}
                              type="button"
                              variant="ghost"
                              aria-label={`Devoluções da venda ${sale.number}`}
                              aria-expanded={panel?.kind === "returns" && panel.id === sale.id}
                              onClick={() => toggle("returns", sale.id)}
                            >
                              Devoluções
                            </Button>
                          )}
                          {canWrite && sale.status !== "cancelled" && (
                            <Button
                              type="button"
                              variant="ghost"
                              className="btn-small"
                              aria-label={`Cancelar a venda ${sale.number}`}
                              aria-expanded={panel?.kind === "cancel" && panel.id === sale.id}
                              onClick={() => toggle("cancel", sale.id)}
                            >
                              Cancelar
                            </Button>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={`Histórico da venda ${sale.number}`}
                            aria-expanded={panel?.kind === "history" && panel.id === sale.id}
                            icon={HistoryIcon}
                            onClick={() => toggle("history", sale.id)}
                          >
                            Histórico
                          </Button>
                        </td>
                      </ListRow>
                      {panel?.id === sale.id && panel.kind === "complete" && (
                        <tr>
                          <td colSpan={6} role="region" aria-label={`Conclusão da venda ${sale.number}`}>
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
                          <td colSpan={6} role="region" aria-label={`Cancelamento da venda ${sale.number}`}>
                            <div className="settlements">
                              <p>
                                {sale.status === "completed"
                                  ? "Cancelar a venda cancela também a conta a receber que ela gerou, e isso não se desfaz: para vender de novo, faça outra venda. Se a conta já recebeu baixas, desfaça as baixas antes."
                                  : "Cancelar o pedido o encerra: ele não é reaberto."}
                              </p>
                              <Notice message={actionError} />
                              <div className="form-actions">
                                <Button type="button" pending={acting} onClick={() => void act(sale, "cancel")}>
                                  Cancelar a venda
                                </Button>
                                <Button type="button" variant="ghost" disabled={acting} onClick={() => setPanel(null)}>
                                  Manter
                                </Button>
                              </div>
                            </div>
                          </td>
                        </tr>
                      )}
                      {panel?.id === sale.id && panel.kind === "returns" && (
                        <tr>
                          <td colSpan={6} role="region" aria-label={`Devoluções da venda ${sale.number}`}>
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
                          <td colSpan={6} role="region" aria-label={`Histórico da venda ${sale.number}`}>
                            <History
                              load={historyLoader(sale.id)}
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
          <Sheet open={editing !== null} locked={saving} title={`Alterar o pedido ${shown?.number ?? ""}`} onClose={closeSheet}>
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
        Concluir gera uma conta a receber de <strong>{formatMoney(sale.total)}</strong>
        {term === null ? ", para o dia da venda" : ` pelo prazo ${term.name}`}. Depois disso os itens não mudam.
      </p>
      {waiting ? (
        <p role="status" className="status">
          <Spinner /> Carregando o prazo…
        </p>
      ) : parts === null ? (
        <Notice message="O valor é pequeno demais para este prazo: uma parcela ficaria com menos de um centavo." />
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
          Concluir venda
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
      setError("Informe a quantidade devolvida de pelo menos um item.");
      return;
    }
    if (!isIsoDate(field("returned_on")) || !isIsoDate(field("refund_due_on"))) {
      setError("Informe a data da devolução e o vencimento do reembolso.");
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
          <Spinner /> Carregando…
        </p>
      ) : returns.length === 0 ? (
        <p className="muted">Nada foi devolvido desta venda.</p>
      ) : (
        <ul className="history">
          {returns.map((made) => (
            <li key={made.id}>
              {formatDate(made.returned_on)} · reembolso de {formatMoney(made.refund)} ·{" "}
              {made.items
                .map((item) => `${sale.lines[item.line - 1]?.description ?? "item"} × ${formatDecimal(item.quantity)}`)
                .join(", ")}
              {made.reason !== null && ` · ${made.reason}`}
              {made.refund_entry_id === null && " · reembolso ainda não lançado"}
            </li>
          ))}
        </ul>
      )}
      {canWrite && (
        <form key={version} className="settle-form" onSubmit={(event) => void submit(event)}>
          <h3>Registrar devolução</h3>
          {sale.lines.map((line, index) => (
            <label key={index}>
              {line.description} (vendido: {formatDecimal(line.quantity)})
              <input
                name={`line-${index + 1}`}
                aria-label={`Quantidade devolvida de ${line.description}`}
                inputMode="decimal"
                autoComplete="off"
                placeholder="0"
              />
            </label>
          ))}
          <label>
            Data da devolução
            <input name="returned_on" type="date" required defaultValue={todayIso()} />
          </label>
          <label>
            Motivo
            <input name="reason" maxLength={200} autoComplete="off" />
          </label>
          <label>
            Categoria do reembolso
            <select name="refund_category_id" required defaultValue="">
              <option value="" disabled>
                Escolha
              </option>
              {categories.map((category) => (
                <option key={category.id} value={category.id}>
                  {category.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Reembolso vence em
            <input name="refund_due_on" type="date" required defaultValue={todayIso()} />
          </label>
          <Notice message={error} />
          <Button type="submit" icon={PlusIcon} pending={saving}>
            Registrar devolução
          </Button>
        </form>
      )}
    </div>
  );
}

/** A value of the trail as a person reads it: names for references, words for keys. */
function displayValue(field: string, value: string | null, names: Map<string, string>): string {
  if (value === null) return "—";
  if (["customer_id", "seller_id", "payment_term_id", "category_id", "cost_center_id"].includes(field)) {
    return names.get(value) ?? value;
  }
  if (field === "status") return SALE_STATUS_LABELS[value as SaleStatus] ?? value;
  if (field === "sold_on") return formatDate(value);
  if (field === "lines") {
    return value
      .split("\n")
      .map((line) => {
        // The description comes last and may hold colons of its own.
        const [product, quantity, price, discount, ...description] = line.split(":");
        const name = description.length > 0 ? description.join(":") : (names.get(product) ?? "item");
        return `${name} × ${quantity.replace(".", ",")} a ${formatMoney(price)}${discount === "0" ? "" : ` (− ${formatMoney(discount)})`}`;
      })
      .join("; ");
  }
  return value;
}
