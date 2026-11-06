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
import { CsvImport } from "../components/CsvImport";
import { PlusIcon } from "../components/icons/plus";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { PageHeader } from "../components/PageHeader";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { ProductForm } from "../components/ProductForm";
import { ProductHistory } from "../components/ProductHistory";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { formatDecimal, formatMoney } from "../lib/decimal";
import { useLastValue } from "../lib/useLastValue";
import { usePagedList } from "../lib/usePagedList";
import { useCanWrite } from "../lib/useCanWrite";
import { SquarePenIcon } from "../components/icons/square-pen";
import { StatusBadge } from "../components/StatusBadge";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function ProductsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("products");
  const [filters, setFilters] = useState<ProductFilters>(NO_PRODUCT_FILTERS);
  const [editing, setEditing] = useState<Product | null>(null);
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

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A product created from the top bar or from here: read the list again and flash the new row.
  useOnCreated("product", (created) => {
    // What an earlier change said is not about this one.
    setSaved(null);
    setHighlightedId(created.id);
    void reload();
  });

  async function handleSubmit(input: ProductInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updateProduct(companyId, editing.id, input);
      // In place: the list stays on the page it was on, and the row stays where it is until the
      // filters are applied again.
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setEditing(null);
      setSaved(`${updated.name} atualizado.`);
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  function closeSheet() {
    // A change on its way finishes first: closing now would hide what a failure has to say.
    if (saving) return;
    setEditing(null);
    setFormError(null);
  }

  function startEditing(product: Product) {
    setEditing(product);
    setFormError(null);
    setSaved(null);
  }

  return (
    <>
      <PageHeader title="Produtos e serviços" description="O que você vende e o que custa para vender." />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("product")}>
            Novo produto ou serviço
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
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
            <p className="muted">Use o formulário para adicionar o primeiro.</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Nome</th>
                  <th className="num">Venda</th>
                  <th className="num">Custo</th>
                  <th className="num">Mínimo</th>
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
                        <div className="cell-title">{product.name}</div>
                        <div className="muted">
                          {[product.sku, product.kind === "product" ? "Produto" : "Serviço", product.unit]
                            .filter(Boolean)
                            .join(" · ")}
                        </div>
                      </td>
                      <td className="num">{formatMoney(product.sale_price)}</td>
                      <td className="num">{formatMoney(product.cost_price)}</td>
                      <td className="num">
                        {product.minimum_stock === null ? "—" : formatDecimal(product.minimum_stock)}
                      </td>
                      <td>
                        <StatusBadge active={product.active} />
                      </td>
                      <td className="row-actions">
                        {canWrite && (
                          <Button
                            iconOnly
                            icon={SquarePenIcon}
                            type="button"
                            variant="ghost"
                            aria-label={`Alterar ${product.name}`}
                            disabled={saving}
                            onClick={() => startEditing(product)}
                          >
                            Alterar
                          </Button>
                        )}
                        <Button
                          iconOnly
                          type="button"
                          variant="ghost"
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
        {canWrite && (
          <Sheet open={editing !== null} title={`Alterar ${shown?.name ?? ""}`} onClose={closeSheet}>
            {shown !== null && (
              <ProductForm
                key={shown.id}
                product={shown}
                pending={saving}
                error={formError}
                onSubmit={(input) => void handleSubmit(input)}
                onCancel={closeSheet}
              />
            )}
          </Sheet>
        )}
      </div>
      {canWrite && (
        <CsvImport
          title="Importar de um arquivo CSV"
          template={PRODUCTS_TEMPLATE}
          templateName="produtos-e-servicos.csv"
          run={(file, dryRun) => api.importProducts(companyId, file, dryRun)}
          onImported={() => void reload()}
        >
          <p>
            A primeira linha dá o nome das colunas, em qualquer ordem; a vírgula ou o ponto e vírgula
            separam. <code>name</code> e <code>unit</code> (como UN ou KG) são obrigatórias. As outras
            são <code>kind</code> (<code>product</code> ou <code>service</code>; em branco é produto),{" "}
            <code>sku</code>, <code>ncm</code>, <code>sale_price</code>, <code>cost_price</code>,{" "}
            <code>minimum_stock</code> e <code>active</code> (<code>sim</code> ou <code>não</code>; em
            branco é ativo). Os preços podem usar a vírgula, como <code>12,50</code>. Um valor com ponto ou vírgula
            seguido de exatamente três dígitos (<code>1.234</code>, <code>1,234</code>) é ambíguo e
            recusado: escreva <code>1234</code>. Serviço não tem <code>ncm</code> nem estoque mínimo.
          </p>
          <p>
            Nada é importado enquanto uma linha tiver problema. Quem já tem o mesmo SKU é ignorado,
            nunca sobrescrito; quem não tem SKU não pode ser reconhecido, então enviar o mesmo arquivo
            duas vezes o repete.
          </p>
        </CsvImport>
      )}
    </>
  );
}

const PRODUCTS_TEMPLATE = [
  "kind,name,sku,unit,ncm,sale_price,cost_price,minimum_stock,active",
  "product,Pão francês,PAO-001,KG,1905.90.90,\"12,50\",7,10,sim",
  "service,Entrega,ENT-001,UN,,5,,,sim",
  "",
].join("\n");
