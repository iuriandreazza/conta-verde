import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_COUNT_FILTERS,
  api,
  errorMessage,
  type InventoryCount,
  type InventoryCountFilters,
  type InventoryCountInput,
  type InventoryCountStatus,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { FileTextIcon } from "../components/icons/file-text";
import { HistoryIcon } from "../components/icons/history";
import { LayersIcon } from "../components/icons/layers";
import { PlusIcon } from "../components/icons/plus";
import { SquarePenIcon } from "../components/icons/square-pen";
import { InventoryCountForm } from "../components/InventoryCountForm";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Sheet } from "../components/Sheet";
import { Spinner } from "../components/Spinner";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { COUNT_STATUS_LABELS, countStatusClass } from "../lib/countStatus";
import { formatDecimal } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";
import { signedQuantity } from "../lib/stock";
import { useCanWrite } from "../lib/useCanWrite";
import { useLastValue } from "../lib/useLastValue";
import { usePagedList } from "../lib/usePagedList";
import { useStockProducts } from "../lib/useStockProducts";

const FIELD_LABELS: Record<string, string> = {
  counted_on: "Data da contagem",
  notes: "Observações",
  lines: "Itens",
  status: "Situação",
};

type PanelKind = "details" | "apply" | "cancel" | "history";
type Panel = { kind: PanelKind; id: string };
type Action = "apply" | "cancel";

const CONFIRMATIONS: Record<Action, { text: string; button: string; done: (count: InventoryCount) => string }> = {
  apply: {
    text: "Aplicar lança no estoque, na data da contagem, um ajuste para cada produto cuja contagem difere do saldo da hora da aplicação. Isso não se desfaz pelo inventário: o inventário fica aplicado e só um novo movimento corrige.",
    button: "Aplicar inventário",
    done: (count) => `Inventário nº ${count.number} aplicado: os ajustes foram lançados no estoque.`,
  },
  cancel: {
    text: "Cancelar encerra o inventário sem mexer no estoque: ele não é reaberto.",
    button: "Cancelar o inventário",
    done: (count) => `Inventário nº ${count.number} cancelado.`,
  },
};

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function InventoryCountsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("purchases");
  const [filters, setFilters] = useState<InventoryCountFilters>(NO_COUNT_FILTERS);
  const [editing, setEditing] = useState<InventoryCount | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [panel, setPanel] = useState<Panel | null>(null);
  const [acting, setActing] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const { products, error: productsError, loaded: productsLoaded } = useStockProducts(companyId);
  const names = useMemo(() => new Map(products.map((product) => [product.id, product.name])), [products]);

  const { items, setItems, nextCursor, loading, loadingMore, error, applied, reload, loadMore } = usePagedList<InventoryCount, InventoryCountFilters>(
    (applied, after) => api.listInventoryCounts(companyId, applied, after),
    filters,
  );
  const isFiltered = applied.q !== "" || applied.status !== "";

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A count made from the top bar or from here: read the list again and flash the new row.
  useOnCreated("inventoryCount", (created) => {
    setSaved(null);
    setHighlightedId(created.id);
    void reload();
  });

  const replace = (updated: InventoryCount) => setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));

  function closeSheet() {
    // A change on its way finishes first: closing now would hide what a failure has to say.
    if (saving) return;
    setEditing(null);
    setFormError(null);
  }

  async function handleSubmit(input: InventoryCountInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updateInventoryCount(companyId, editing.id, input);
      replace(updated);
      setEditing(null);
      setSaved(`Inventário nº ${updated.number} atualizado.`);
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  async function act(count: InventoryCount, action: Action) {
    if (acting) return;
    setActing(true);
    setActionError(null);
    setSaved(null);
    try {
      const updated = await (action === "apply" ? api.applyInventoryCount : api.cancelInventoryCount)(companyId, count.id);
      replace(updated);
      setPanel(null);
      setSaved(CONFIRMATIONS[action].done(updated));
    } catch (failure) {
      setActionError(errorMessage(failure));
    } finally {
      setActing(false);
    }
  }

  const historyLoader = useCallback((id: string) => () => api.inventoryCountHistory(companyId, id), [companyId]);
  const toggle = (kind: PanelKind, id: string) => {
    setPanel(panel?.kind === kind && panel.id === id ? null : { kind, id });
    setActionError(null);
  };
  const isOpen = (kind: PanelKind, id: string) => panel?.kind === kind && panel.id === id;

  return (
    <>
      <PageHeader
        title="Inventários"
        description="Contagens do que há nas prateleiras: aplicar uma contagem ajusta o estoque às quantidades encontradas."
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("inventoryCount")}>
            Novo inventário
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
          <h2>Contagens</h2>
          <div className="filters" role="search">
            <label>
              Buscar
              <input
                type="search"
                value={filters.q}
                placeholder="Número ou observação"
                autoComplete="off"
                maxLength={100}
                onChange={(event) => setFilters({ ...filters, q: event.target.value })}
              />
            </label>
            <label>
              Situação
              <select value={filters.status} onChange={(event) => setFilters({ ...filters, status: event.target.value as InventoryCountFilters["status"] })}>
                <option value="">Todas</option>
                {Object.entries(COUNT_STATUS_LABELS).map(([key, label]) => (
                  <option key={key} value={key}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <Notice message={error ?? productsError} />
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
              <LayersIcon size={40} aria-hidden />
              <p role="status">Nenhum inventário encontrado.</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_COUNT_FILTERS })}>
                Limpar filtros
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <LayersIcon size={40} aria-hidden />
              <h3>Realize um inventário para atualizar seu estoque</h3>
              <p className="muted">
                Compare o estoque físico com o do sistema: informe o que foi contado e o sistema ajusta as diferenças, ao custo médio, na data da contagem.
              </p>
              {canWrite && (
                <Button type="button" icon={PlusIcon} onClick={() => openCreate("inventoryCount")}>
                  Novo inventário
                </Button>
              )}
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Nº</th>
                    <th>Data</th>
                    <th>Produtos</th>
                    <th>Situação</th>
                    <th>
                      <span className="visually-hidden">Ações</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((count, index) => (
                    <Fragment key={count.id}>
                      <ListRow index={index} highlighted={count.id === highlightedId}>
                        <td>{count.number}</td>
                        <td>
                          <div className="cell-title">{formatDate(count.counted_on)}</div>
                          {count.notes !== null && <div className="muted">{count.notes}</div>}
                        </td>
                        <td>
                          {count.lines.length} {count.lines.length === 1 ? "produto" : "produtos"}
                        </td>
                        <td>
                          <span className={countStatusClass(count.status)}>{COUNT_STATUS_LABELS[count.status]}</span>
                        </td>
                        <td className="row-actions">
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            icon={FileTextIcon}
                            aria-label={`Detalhes do inventário ${count.number}`}
                            aria-expanded={isOpen("details", count.id)}
                            onClick={() => toggle("details", count.id)}
                          >
                            Detalhes
                          </Button>
                          {canWrite && count.status === "open" && (
                            <>
                              <Button
                                iconOnly
                                icon={SquarePenIcon}
                                type="button"
                                variant="ghost"
                                aria-label={`Alterar o inventário ${count.number}`}
                                disabled={saving || acting}
                                onClick={() => {
                                  setEditing(count);
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
                                aria-label={`Aplicar o inventário ${count.number}`}
                                aria-expanded={isOpen("apply", count.id)}
                                onClick={() => toggle("apply", count.id)}
                              >
                                Aplicar
                              </Button>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={`Cancelar o inventário ${count.number}`}
                                aria-expanded={isOpen("cancel", count.id)}
                                onClick={() => toggle("cancel", count.id)}
                              >
                                Cancelar
                              </Button>
                            </>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={`Histórico do inventário ${count.number}`}
                            aria-expanded={isOpen("history", count.id)}
                            icon={HistoryIcon}
                            onClick={() => toggle("history", count.id)}
                          >
                            Histórico
                          </Button>
                        </td>
                      </ListRow>
                      {panel?.id === count.id && panel.kind === "details" && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={`Detalhes do inventário ${count.number}`}>
                            <DetailsPanel companyId={companyId} count={count} names={names} />
                          </td>
                        </tr>
                      )}
                      {panel?.id === count.id && (panel.kind === "apply" || panel.kind === "cancel") && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={`Confirmação do inventário ${count.number}`}>
                            <div className="settlements">
                              <p>{CONFIRMATIONS[panel.kind].text}</p>
                              <Notice message={actionError} />
                              <div className="form-actions">
                                <Button type="button" pending={acting} onClick={() => void act(count, panel.kind as Action)}>
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
                      {panel?.id === count.id && panel.kind === "history" && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={`Histórico do inventário ${count.number}`}>
                            <History
                              load={historyLoader(count.id)}
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
          <Sheet open={editing !== null} locked={saving} title={`Alterar o inventário ${shown?.number ?? ""}`} onClose={closeSheet}>
            {shown !== null &&
              (productsLoaded ? (
                <InventoryCountForm
                  key={shown.id}
                  count={shown}
                  products={products}
                  pending={saving}
                  error={formError}
                  onSubmit={(input) => void handleSubmit(input)}
                  onCancel={closeSheet}
                />
              ) : productsError === null ? (
                <p role="status" className="status">
                  <Spinner /> Carregando…
                </p>
              ) : (
                <p role="alert">{productsError}</p>
              ))}
          </Sheet>
        )}
      </div>
    </>
  );
}

const isZero = (quantity: string) => /^-?0(\.0+)?$/.test(quantity);

/** What each product was found at, what the stock holds (or held, once applied) and the difference. */
function DetailsPanel({ companyId, count, names }: { companyId: string; count: InventoryCount; names: Map<string, string> }) {
  const [detail, setDetail] = useState<InventoryCount | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    api
      .getInventoryCount(companyId, count.id)
      .then((reply) => current && setDetail(reply))
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId, count.id, count.status]);

  return (
    <div className="settlements">
      <Notice message={error} />
      {detail === null ? (
        error === null && (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        )
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Produto</th>
                <th className="num">Contado</th>
                <th className="num">{detail.status === "applied" ? "Em estoque na aplicação" : "Em estoque"}</th>
                <th className="num">Diferença</th>
              </tr>
            </thead>
            <tbody>
              {detail.lines.map((line) => (
                <tr key={line.product_id}>
                  <td>{names.get(line.product_id) ?? ""}</td>
                  <td className="num">{formatDecimal(line.counted)}</td>
                  <td className="num">{line.expected === null ? "—" : formatDecimal(line.expected)}</td>
                  <td className="num">
                    {line.difference === null ? (
                      "—"
                    ) : isZero(line.difference) ? (
                      "0"
                    ) : (
                      <strong className={line.difference.startsWith("-") ? "negative" : undefined}>
                        {line.difference.startsWith("-") ? "▼" : "▲"} {signedQuantity(formatDecimal(line.difference))}
                      </strong>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

/** A value of the trail as a person reads it: words for keys. */
function displayValue(field: string, value: string | null, names: Map<string, string>): string {
  if (value === null) return "—";
  if (field === "status") return COUNT_STATUS_LABELS[value as InventoryCountStatus] ?? value;
  if (field === "counted_on") return formatDate(value);
  if (field === "lines") {
    // One line per row, as `product:counted`.
    return value
      .split("\n")
      .map((line) => {
        const [product, counted = ""] = line.split(":");
        return `${names.get(product) ?? "produto"} × ${formatDecimal(counted)}`;
      })
      .join("; ");
  }
  return value;
}
