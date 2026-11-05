import { Fragment, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_PRODUCT_FILTERS,
  api,
  errorMessage,
  type Product,
  type ProductFilters,
  type ProductInput,
} from "../api";
import { Button } from "../components/Button";
import { CompanyNav } from "../components/CompanyNav";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { ProductForm } from "../components/ProductForm";
import { ProductHistory } from "../components/ProductHistory";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { formatDecimal, formatMoney } from "../lib/decimal";
import { usePagedList } from "../lib/usePagedList";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function ProductsPage() {
  const { companyId = "" } = useParams();
  const [filters, setFilters] = useState<ProductFilters>(NO_PRODUCT_FILTERS);
  const [editing, setEditing] = useState<Product | null>(null);
  // Bumped after a registration so the form starts empty again.
  const [formVersion, setFormVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);

  const {
    items: products,
    setItems,
    nextCursor,
    loading,
    loadingMore,
    error,
    applied,
    reload,
    loadMore,
  } = usePagedList<Product, ProductFilters>(
    (applied, after) => api.listProducts(companyId, applied, after),
    filters,
  );
  const isFiltered = applied.q !== "" || applied.kind !== "" || applied.active !== "";

  async function handleSubmit(input: ProductInput) {
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      if (editing === null) {
        const created = await api.createProduct(companyId, input);
        setHighlightedId(created.id);
        setFormVersion((version) => version + 1);
        setSaved(`${created.name} cadastrado.`);
      } else {
        const updated = await api.updateProduct(companyId, editing.id, input);
        // In place: the list stays on the page it was on, and the row stays where it is until the
        // filters are applied again.
        setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
        setEditing(null);
        setSaved(`${updated.name} atualizado.`);
        setFormVersion((version) => version + 1);
        return;
      }
      await reload();
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  function startEditing(product: Product) {
    setEditing(product);
    setFormError(null);
    setSaved(null);
  }

  return (
    <>
      <h1>Produtos e serviços</h1>
      <CompanyNav companyId={companyId} />

      <section className="card">
        <h2>{editing === null ? "Novo produto ou serviço" : `Alterar ${editing.name}`}</h2>
        <ProductForm
          // A new key starts the form over for another product, or an empty one after a save.
          key={`${editing?.id ?? "new"}-${formVersion}`}
          product={editing}
          // After a save or when editing, the person is in the form: put the cursor back in it.
          focusName={editing !== null || formVersion > 0}
          pending={saving}
          error={formError}
          onSubmit={(input) => void handleSubmit(input)}
          onCancel={() => {
            setEditing(null);
            setFormError(null);
          }}
        />
        <Notice message={saved} tone="success" />
      </section>

      <section className="card">
        <h2>Cadastrados</h2>
        <div className="filters" role="search">
          <label>
            Buscar
            <input
              type="search"
              value={filters.q}
              placeholder="Nome ou SKU"
              autoComplete="off"
              maxLength={100}
              onChange={(event) => setFilters({ ...filters, q: event.target.value })}
            />
          </label>
          <label>
            Mostrar
            <select
              value={filters.kind}
              onChange={(event) =>
                setFilters({ ...filters, kind: event.target.value as ProductFilters["kind"] })
              }
            >
              <option value="">Todos</option>
              <option value="product">Produtos</option>
              <option value="service">Serviços</option>
            </select>
          </label>
          <label>
            Situação
            <select
              value={filters.active}
              onChange={(event) =>
                setFilters({ ...filters, active: event.target.value as ProductFilters["active"] })
              }
            >
              <option value="">Ativos e inativos</option>
              <option value="true">Só ativos</option>
              <option value="false">Só inativos</option>
            </select>
          </label>
          <label>
            Ordem
            <select
              value={filters.sort}
              onChange={(event) =>
                setFilters({ ...filters, sort: event.target.value as ProductFilters["sort"] })
              }
            >
              <option value="name">Nome (A–Z)</option>
              <option value="-name">Nome (Z–A)</option>
            </select>
          </label>
        </div>
        <Notice message={error} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        ) : products.length === 0 && error ? (
          // The list could not be loaded at all: offer a retry instead of a table with no rows.
          <Button type="button" variant="ghost" onClick={() => void reload()}>
            Tentar de novo
          </Button>
        ) : products.length === 0 && isFiltered ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p role="status">Nenhum cadastro encontrado.</p>
            <Button
              type="button"
              variant="ghost"
              onClick={() => setFilters({ ...NO_PRODUCT_FILTERS, sort: filters.sort })}
            >
              Limpar filtros
            </Button>
          </div>
        ) : products.length === 0 ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p>Nenhum produto ou serviço ainda.</p>
            <p className="muted">Use o formulário acima para adicionar o primeiro.</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Nome</th>
                  <th>Tipo</th>
                  <th>Un.</th>
                  <th>Venda</th>
                  <th>Custo</th>
                  <th>Mínimo</th>
                  <th>Situação</th>
                  <th>
                    <span className="visually-hidden">Ações</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {products.map((product, index) => (
                  <Fragment key={product.id}>
                    <ListRow index={index} highlighted={product.id === highlightedId}>
                      <td>
                        {product.name}
                        {product.sku && <span className="muted sku"> {product.sku}</span>}
                      </td>
                      <td>{product.kind === "product" ? "Produto" : "Serviço"}</td>
                      <td>{product.unit}</td>
                      <td>{formatMoney(product.sale_price)}</td>
                      <td>{formatMoney(product.cost_price)}</td>
                      <td>
                        {product.minimum_stock === null ? "—" : formatDecimal(product.minimum_stock)}
                      </td>
                      <td>{product.active ? "Ativo" : "Inativo"}</td>
                      <td>
                        <Button
                          type="button"
                          variant="ghost"
                          className="btn-small"
                          aria-label={`Alterar ${product.name}`}
                          disabled={saving}
                          onClick={() => startEditing(product)}
                        >
                          Alterar
                        </Button>
                        <Button
                          type="button"
                          variant="ghost"
                          className="btn-small"
                          aria-label={`Histórico de ${product.name}`}
                          aria-expanded={historyOpenFor === product.id}
                          aria-controls={`history-${product.id}`}
                          icon={HistoryIcon}
                          onClick={() =>
                            setHistoryOpenFor(historyOpenFor === product.id ? null : product.id)
                          }
                        >
                          Histórico
                        </Button>
                      </td>
                    </ListRow>
                    {historyOpenFor === product.id && (
                      <tr id={`history-${product.id}`}>
                        <td colSpan={8} role="region" aria-label={`Histórico de ${product.name}`}>
                          <ProductHistory
                            // Remounted when the product changes, so an open history follows an edit.
                            key={JSON.stringify(product)}
                            companyId={companyId}
                            productId={product.id}
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
          <Button
            type="button"
            variant="ghost"
            icon={ChevronDownIcon}
            pending={loadingMore}
            onClick={() => void loadMore()}
          >
            Carregar mais
          </Button>
        )}
      </section>
    </>
  );
}
