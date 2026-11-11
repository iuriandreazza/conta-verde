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
import { ListTools } from "../components/ListTools";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { ProductForm } from "../components/ProductForm";
import { ProductHistory } from "../components/ProductHistory";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { formatDecimal, formatMoney, toInputDecimal } from "../lib/decimal";
import { useLastValue } from "../lib/useLastValue";
import { usePagedList } from "../lib/usePagedList";
import { useCanWrite } from "../lib/useCanWrite";
import { msg, useI18n, type Translate } from "../i18n";
import { SquarePenIcon } from "../components/icons/square-pen";
import { StatusBadge } from "../components/StatusBadge";

const PRODUCT_COLUMNS = [msg("Type"), msg("Name"), msg("SKU"), msg("Unit"), msg("NCM"), msg("Sale price"), msg("Cost"), msg("Minimum stock"), msg("Status")];

function productRow(product: Product, t: Translate): string[] {
  return [
    product.kind === "service" ? t("Service") : t("Product"),
    product.name,
    product.sku ?? "",
    product.unit,
    product.ncm ?? "",
    toInputDecimal(product.sale_price),
    toInputDecimal(product.cost_price),
    toInputDecimal(product.minimum_stock),
    product.active ? t("Active") : t("Inactive"),
  ];
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function ProductsPage() {
  const { t, rich } = useI18n();
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
      setSaved(t("{name} updated.", { name: updated.name }));
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
      <PageHeader title={t("Products and services")} description={t("What you sell and what it costs to sell it.")} />
      <div className="list-actions">
        {canWrite && (
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("product")}>
            {t("New product or service")}
          </Button>
        )}
        <ListTools
          filename="produtos-e-servicos"
          header={PRODUCT_COLUMNS.map((column) => t(column))}
          fetchPage={(after) => api.listProducts(companyId, applied, after)}
          toRow={(product) => productRow(product, t)}
        />
      </div>
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
        <h2>{t("Registered")}</h2>
        <div className="filters" role="search">
          <label>
            {t("Search")}
            <input
              type="search"
              value={filters.q}
              placeholder={t("Name or SKU")}
              autoComplete="off"
              maxLength={100}
              onChange={(event) => setFilters({ ...filters, q: event.target.value })}
            />
          </label>
          <label>
            {t("Show")}
            <select
              value={filters.kind}
              onChange={(event) =>
                setFilters({ ...filters, kind: event.target.value as ProductFilters["kind"] })
              }
            >
              <option value="">{t("All")}</option>
              <option value="product">{t("Products")}</option>
              <option value="service">{t("Services")}</option>
            </select>
          </label>
          <label>
            {t("Status")}
            <select
              value={filters.active}
              onChange={(event) =>
                setFilters({ ...filters, active: event.target.value as ProductFilters["active"] })
              }
            >
              <option value="">{t("Active and inactive")}</option>
              <option value="true">{t("Active only")}</option>
              <option value="false">{t("Inactive only")}</option>
            </select>
          </label>
          <label>
            {t("Order")}
            <select
              value={filters.sort}
              onChange={(event) =>
                setFilters({ ...filters, sort: event.target.value as ProductFilters["sort"] })
              }
            >
              <option value="name">{t("Name (A–Z)")}</option>
              <option value="-name">{t("Name (Z–A)")}</option>
            </select>
          </label>
        </div>
        <Notice message={error} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        ) : products.length === 0 && error ? (
          // The list could not be loaded at all: offer a retry instead of a table with no rows.
          <Button type="button" variant="ghost" onClick={() => void reload()}>
            {t("Try again")}
          </Button>
        ) : products.length === 0 && isFiltered ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p role="status">{t("No records found.")}</p>
            <Button
              type="button"
              variant="ghost"
              onClick={() => setFilters({ ...NO_PRODUCT_FILTERS, sort: filters.sort })}
            >
              {t("Clear filters")}
            </Button>
          </div>
        ) : products.length === 0 ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p>{t("No products or services yet.")}</p>
            <p className="muted">{t("Use the form to add the first one.")}</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Name")}</th>
                  <th className="num">{t("Sale")}</th>
                  <th className="num">{t("Cost")}</th>
                  <th className="num">{t("Minimum")}</th>
                  <th>{t("Status")}</th>
                  <th>
                    <span className="visually-hidden">{t("Actions")}</span>
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
                          {[product.sku, product.kind === "product" ? t("Product") : t("Service"), product.unit]
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
                            aria-label={t("Edit {name}", { name: product.name })}
                            disabled={saving}
                            onClick={() => startEditing(product)}
                          >
                            {t("Edit")}
                          </Button>
                        )}
                        <Button
                          iconOnly
                          type="button"
                          variant="ghost"
                          aria-label={t("History of {name}", { name: product.name })}
                          aria-expanded={historyOpenFor === product.id}
                          aria-controls={`history-${product.id}`}
                          icon={HistoryIcon}
                          onClick={() =>
                            setHistoryOpenFor(historyOpenFor === product.id ? null : product.id)
                          }
                        >
                          {t("History")}
                        </Button>
                      </td>
                    </ListRow>
                    {historyOpenFor === product.id && (
                      <tr id={`history-${product.id}`}>
                        <td colSpan={8} role="region" aria-label={t("History of {name}", { name: product.name })}>
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
            {t("Load more")}
          </Button>
        )}
      </section>
        {canWrite && (
          <Sheet open={editing !== null} locked={saving} title={t("Edit {name}", { name: shown?.name ?? "" })} onClose={closeSheet}>
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
          title={t("Import from a CSV file")}
          template={PRODUCTS_TEMPLATE}
          templateName="produtos-e-servicos.csv"
          run={(file, dryRun) => api.importProducts(companyId, file, dryRun)}
          onImported={() => void reload()}
        >
          <p>
            {rich(
              "The first line names the columns, in any order; a comma or a semicolon separates them. {name} and {unit} (such as UN or KG) are required. The others are {kind} ({product} or {service}; blank means product), {sku}, {ncm}, {sale_price}, {cost_price}, {minimum_stock} and {active} ({yes} or {no}; blank means active). Prices may use a comma, like {example}. A value with a period or a comma followed by exactly three digits ({thousands_dot}, {thousands_comma}) is ambiguous and refused: write {plain}. A service has no {ncm} or minimum stock.",
              {
                name: <code>name</code>,
                unit: <code>unit</code>,
                kind: <code>kind</code>,
                product: <code>product</code>,
                service: <code>service</code>,
                sku: <code>sku</code>,
                ncm: <code>ncm</code>,
                sale_price: <code>sale_price</code>,
                cost_price: <code>cost_price</code>,
                minimum_stock: <code>minimum_stock</code>,
                active: <code>active</code>,
                yes: <code>sim</code>,
                no: <code>não</code>,
                example: <code>12,50</code>,
                thousands_dot: <code>1.234</code>,
                thousands_comma: <code>1,234</code>,
                plain: <code>1234</code>,
              },
            )}
          </p>
          <p>
            {t("Nothing is imported while a row has a problem. Anyone with the same SKU is skipped, never overwritten; anyone with no SKU cannot be recognized, so sending the same file twice repeats it.")}
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
