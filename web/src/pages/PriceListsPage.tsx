import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_PRICE_LIST_FILTERS,
  NO_PRODUCT_FILTERS,
  api,
  errorMessage,
  type PriceList,
  type PriceListFilters,
  type PriceListInput,
  type Product,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { SquarePenIcon } from "../components/icons/square-pen";
import { LayersIcon } from "../components/icons/layers";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PriceListForm } from "../components/PriceListForm";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { useLastValue } from "../lib/useLastValue";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { StatusBadge } from "../components/StatusBadge";
import { formatMoney } from "../lib/decimal";
import { readUpTo } from "../lib/readUpTo";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

const FIELD_LABELS: Record<string, string> = { name: "Nome", items: "Preços", active: "Situação" };

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function PriceListsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("sales");
  const [filters, setFilters] = useState<PriceListFilters>(NO_PRICE_LIST_FILTERS);
  const [editing, setEditing] = useState<PriceList | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOf, setHistoryOf] = useState<string | null>(null);
  const [products, setProducts] = useState<Product[]>([]);
  const [targetsError, setTargetsError] = useState<string | null>(null);
  const [targetsLoaded, setTargetsLoaded] = useState(false);

  useEffect(() => {
    let current = true;
    readUpTo((after) => api.listProducts(companyId, NO_PRODUCT_FILTERS, after))
      .then((loaded) => {
        if (current) {
          setProducts(loaded);
          setTargetsLoaded(true);
        }
      })
      .catch((failure: unknown) => current && setTargetsError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  const names = useMemo(() => new Map(products.map((product) => [product.id, product.name])), [products]);

  const { items, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<PriceList, PriceListFilters>((applied, after) => api.listPriceLists(companyId, applied, after), filters);
  const isFiltered = applied.q !== "" || applied.active !== "";

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A table made from the top bar or from here: read the list again and flash the new row.
  useOnCreated("priceList", (created) => {
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

  async function handleSubmit(input: PriceListInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updatePriceList(companyId, editing.id, input);
      setHighlightedId(updated.id);
      setEditing(null);
      setSaved(`${updated.name} atualizada. Documentos já feitos guardam os preços que tinham.`);
      await reload();
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  const historyLoader = useCallback((id: string) => () => api.priceListHistory(companyId, id), [companyId]);

  return (
    <>
      <PageHeader
        title="Tabelas de preço"
        description="Preços de venda por produto para atacado, varejo ou um parceiro. Escolher uma tabela num orçamento, venda ou contrato só preenche o preço dos itens."
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("priceList")}>
            Nova tabela de preço
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
          <h2>Cadastradas</h2>
          <div className="filters" role="search">
            <label>
              Buscar
              <input
                type="search"
                value={filters.q}
                placeholder="Nome"
                autoComplete="off"
                maxLength={100}
                onChange={(event) => setFilters({ ...filters, q: event.target.value })}
              />
            </label>
            <label>
              Situação
              <select
                value={filters.active}
                onChange={(event) => setFilters({ ...filters, active: event.target.value as PriceListFilters["active"] })}
              >
                <option value="">Ativas e inativas</option>
                <option value="true">Só ativas</option>
                <option value="false">Só inativas</option>
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
              <LayersIcon size={40} aria-hidden />
              <p role="status">Nenhuma tabela encontrada.</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_PRICE_LIST_FILTERS })}>
                Limpar filtros
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <LayersIcon size={40} aria-hidden />
              <p>Nenhuma tabela de preço ainda.</p>
              <p className="muted">Sem tabela, cada item usa o preço de venda do produto.</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Nome</th>
                    <th className="num">Preços</th>
                    <th>Situação</th>
                    <th>
                      <span className="visually-hidden">Ações</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((list, index) => (
                    <Fragment key={list.id}>
                      <ListRow index={index} highlighted={list.id === highlightedId}>
                        <td className="cell-title">{list.name}</td>
                        <td className="num">{list.items.length}</td>
                        <td>
                          <StatusBadge active={list.active} feminine />
                        </td>
                        <td className="row-actions">
                          {canWrite && (
                            <Button
                              iconOnly
                              icon={SquarePenIcon}
                              type="button"
                              variant="ghost"
                              aria-label={`Alterar ${list.name}`}
                              disabled={saving}
                              onClick={() => {
                                setEditing(list);
                                setFormError(null);
                                setSaved(null);
                              }}
                            >
                              Alterar
                            </Button>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={`Histórico de ${list.name}`}
                            aria-expanded={historyOf === list.id}
                            icon={HistoryIcon}
                            onClick={() => setHistoryOf(historyOf === list.id ? null : list.id)}
                          >
                            Histórico
                          </Button>
                        </td>
                      </ListRow>
                      {historyOf === list.id && (
                        <tr>
                          <td colSpan={4} role="region" aria-label={`Histórico de ${list.name}`}>
                            <History
                              key={JSON.stringify(list)}
                              load={historyLoader(list.id)}
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
        </section>
        {canWrite && (
          <Sheet open={editing !== null} locked={saving} title={`Alterar ${shown?.name ?? ""}`} onClose={closeSheet}>
            {shown !== null &&
              (targetsLoaded ? (
                <PriceListForm
                  key={shown.id}
                  list={shown}
                  products={products}
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

/** A value of the trail as a person reads it: a product by name and its price in reais. */
function displayValue(field: string, value: string | null, names: Map<string, string>): string {
  if (value === null) return "—";
  if (field === "active") return value === "true" ? "Ativa" : "Inativa";
  if (field === "items") {
    return value
      .split("\n")
      .map((row) => {
        const [product, price] = row.split(":");
        return `${names.get(product) ?? "produto"} a ${formatMoney(price)}`;
      })
      .join("; ");
  }
  return value;
}
