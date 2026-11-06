import { Fragment, useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
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
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { SquarePenIcon } from "../components/icons/square-pen";
import { LayersIcon } from "../components/icons/layers";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { StatusBadge } from "../components/StatusBadge";
import { formatMoney, isAmbiguousDecimal, toApiDecimal, toInputDecimal } from "../lib/decimal";
import { readUpTo } from "../lib/readUpTo";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

/** The most prices a list holds (the API's bound). */
const MAX_PRICES = 1000;

const FIELD_LABELS: Record<string, string> = { name: "Nome", items: "Preços", active: "Situação" };

/** A price as it is typed: both fields are text until the form is sent. */
interface Row {
  key: number;
  product_id: string;
  price: string;
}

let lastKey = 0;
const newRow = (product_id = "", price = ""): Row => {
  lastKey += 1;
  return { key: lastKey, product_id, price };
};

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function PriceListsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("sales");
  const [filters, setFilters] = useState<PriceListFilters>(NO_PRICE_LIST_FILTERS);
  const [editing, setEditing] = useState<PriceList | null>(null);
  const [formVersion, setFormVersion] = useState(0);
  const [rows, setRows] = useState<Row[]>(() => [newRow()]);
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

  const inList = new Set(rows.map((row) => row.product_id));
  const productChoices = products.filter((product) => product.active || inList.has(product.id));
  const missing = products.filter((product) => product.active && product.sale_price !== null && !inList.has(product.id));

  function startEditing(list: PriceList | null) {
    setEditing(list);
    setRows(
      list === null ? [newRow()] : list.items.map((item) => newRow(item.product_id, toInputDecimal(item.price))),
    );
    setFormError(null);
    setSaved(null);
    setFormVersion((version) => version + 1);
  }

  const change = (key: number, patch: Partial<Row>) =>
    setRows((current) => current.map((row) => (row.key === key ? { ...row, ...patch } : row)));

  function chooseProduct(key: number, productId: string) {
    const product = products.find((item) => item.id === productId);
    // The sale price is a starting point, and an empty field is easier to fill than a wrong one is to notice.
    change(key, { product_id: productId, price: product?.sale_price ? toInputDecimal(product.sale_price) : "" });
  }

  function addAllProducts() {
    setRows((current) => [
      ...current.filter((row) => row.product_id !== "" || row.price !== ""),
      ...missing.map((product) => newRow(product.id, toInputDecimal(product.sale_price))),
    ]);
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const position = rows.findIndex((row) => row.product_id === "");
    if (position !== -1) {
      setFormError(`Escolha o produto do item ${position + 1}.`);
      return;
    }
    if (new Set(rows.map((row) => row.product_id)).size !== rows.length) {
      setFormError("Cada produto aparece uma vez só na tabela.");
      return;
    }
    const wrong = rows.findIndex((row) => isAmbiguousDecimal(row.price) || toApiDecimal(row.price) === undefined);
    if (wrong !== -1) {
      setFormError(`Informe o preço do item ${wrong + 1}, por exemplo 12,50.`);
      return;
    }
    const form = new FormData(event.currentTarget);
    const input: PriceListInput = {
      name: String(form.get("name") ?? "").trim(),
      items: rows.map((row) => ({ product_id: row.product_id, price: toApiDecimal(row.price) ?? "" })),
      active: form.get("active") === "on",
    };
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      if (editing === null) {
        const created = await api.createPriceList(companyId, input);
        setHighlightedId(created.id);
        startEditing(null);
        setSaved(`${created.name} cadastrada.`);
        await reload();
      } else {
        const updated = await api.updatePriceList(companyId, editing.id, input);
        setHighlightedId(updated.id);
        startEditing(null);
        setSaved(`${updated.name} atualizada. Documentos já feitos guardam os preços que tinham.`);
        await reload();
      }
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
      <div className={canWrite ? "split" : "split split-single"}>
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
                              onClick={() => startEditing(list)}
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
          <aside className="card form-card">
            <h2>{editing === null ? "Nova tabela de preço" : `Alterar ${editing.name}`}</h2>
            {targetsLoaded && (
              <form key={`${editing?.id ?? "new"}-${formVersion}`} onSubmit={(event) => void handleSubmit(event)}>
                <label>
                  Nome
                  <input
                    name="name"
                    required
                    maxLength={200}
                    autoComplete="off"
                    placeholder="Por exemplo Atacado"
                    defaultValue={editing?.name}
                  />
                </label>
                <fieldset className="lines">
                  <legend>Preços</legend>
                  {rows.map((row, index) => (
                    <div className="line-row" key={row.key}>
                      <label className="span-all">
                        Produto ou serviço
                        <select
                          aria-label={`Produto ou serviço do preço ${index + 1}`}
                          value={row.product_id}
                          onChange={(event) => chooseProduct(row.key, event.target.value)}
                        >
                          <option value="" disabled>
                            Escolha
                          </option>
                          {productChoices.map((product) => (
                            <option key={product.id} value={product.id} disabled={inList.has(product.id) && product.id !== row.product_id}>
                              {product.name}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label>
                        Preço
                        <input
                          aria-label={`Preço do item ${index + 1}`}
                          inputMode="decimal"
                          autoComplete="off"
                          placeholder="0,00"
                          value={row.price}
                          onChange={(event) => change(row.key, { price: event.target.value })}
                        />
                      </label>
                      <Button
                        type="button"
                        variant="ghost"
                        className="btn-small"
                        aria-label={`Remover o preço ${index + 1}`}
                        disabled={rows.length === 1}
                        onClick={() => setRows((current) => current.filter((item) => item.key !== row.key))}
                      >
                        Remover
                      </Button>
                    </div>
                  ))}
                  <div className="form-actions">
                    <Button
                      type="button"
                      variant="ghost"
                      className="btn-small"
                      icon={PlusIcon}
                      disabled={rows.length >= MAX_PRICES}
                      onClick={() => setRows((current) => [...current, newRow()])}
                    >
                      Adicionar produto
                    </Button>
                    <Button type="button" variant="ghost" className="btn-small" disabled={missing.length === 0} onClick={addAllProducts}>
                      Incluir todos os produtos
                    </Button>
                  </div>
                  <p className="muted">Incluir todos traz o preço de venda de cada produto que ainda não está na tabela.</p>
                </fieldset>
                <label className="inline">
                  <input name="active" type="checkbox" defaultChecked={editing?.active ?? true} /> Ativa
                </label>
                <Notice message={formError} />
                <div className="form-actions">
                  <Button type="submit" icon={editing === null ? PlusIcon : CheckIcon} pending={saving}>
                    {editing === null ? "Cadastrar tabela" : "Salvar alterações"}
                  </Button>
                  {editing !== null && (
                    <Button type="button" variant="ghost" disabled={saving} onClick={() => startEditing(null)}>
                      Cancelar edição
                    </Button>
                  )}
                </div>
              </form>
            )}
            <Notice message={saved} tone="success" />
          </aside>
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
