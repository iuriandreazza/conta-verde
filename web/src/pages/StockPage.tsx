import { Fragment, useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import {
  NO_MOVEMENT_FILTERS,
  NO_POSITION_FILTERS,
  api,
  errorMessage,
  type Product,
  type StockAlert,
  type StockMovement,
  type StockMovementFilters,
  type StockMovementKind,
  type StockOriginKind,
  type StockPosition,
  type StockPositionFilters,
  type StockSettings,
} from "../api";
import { Button } from "../components/Button";
import { CsvImport } from "../components/CsvImport";
import { FilterTiles, type TileOption } from "../components/FilterTiles";
import { History } from "../components/History";
import { BoxIcon } from "../components/icons/box";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { PlusIcon } from "../components/icons/plus";
import { RotateCCWIcon } from "../components/icons/rotate-ccw";
import { ColumnChooser } from "../components/ColumnChooser";
import { ListRow } from "../components/ListRow";
import { ListTools } from "../components/ListTools";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { msg, useI18n, useT, type Translate } from "../i18n";
import { formatDecimal, formatMoney, toInputDecimal } from "../lib/decimal";
import { formatDate, isIsoDate, wholeDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { isNegative, signedQuantity, trimCost } from "../lib/stock";
import { useCanWrite } from "../lib/useCanWrite";
import { useColumns, type ColumnDef } from "../lib/useColumns";
import { useCounts } from "../lib/useCounts";
import { usePagedList } from "../lib/usePagedList";
import { useStockProducts } from "../lib/useStockProducts";
import { useParams } from "react-router-dom";

type View = "positions" | "movements" | "alerts" | "settings" | "import";

const KIND_LABELS: Record<StockMovementKind, string> = {
  in: msg("Stock entry"),
  out: msg("Exit"),
  adjustment: msg("Adjustment"),
};

const ORIGIN_LABELS: Record<StockOriginKind, string> = {
  purchase_receipt: msg("Purchase"),
  sale: msg("Sale"),
  sale_return: msg("Return"),
  inventory_count: msg("Inventory count"),
  import: msg("Opening balance"),
};

/** A date field that is not a whole date yet (a year still being typed) asks nothing of the API. */

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function StockPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("purchases");
  const { openCreate } = useCreation();
  const [view, setView] = useState<View>("positions");
  const { products, error: productsError } = useStockProducts(companyId);
  const [alerts, setAlerts] = useState<StockAlert[] | null>(null);
  const [alertsError, setAlertsError] = useState<string | null>(null);

  // Read on entry and again when a movement is written: what falls below the minimum is news to the whole page.
  const loadAlerts = useCallback(() => {
    api
      .getStockAlerts(companyId)
      .then((reply) => {
        setAlerts(reply.items);
        setAlertsError(null);
      })
      .catch((failure: unknown) => setAlertsError(errorMessage(failure)));
  }, [companyId]);
  useEffect(loadAlerts, [loadAlerts]);
  useOnCreated("stockMovement", loadAlerts);

  return (
    <>
        <PageHeader
          title={t("Stock")}
          description={t("What is on hand of each product, at what average cost, and the ledger of movements: purchases, sales and returns come in on their own; adjustments and one-off entries are made here.")}
        />
        {canWrite && (
          <div className="list-actions">
            <Button type="button" icon={PlusIcon} onClick={() => openCreate("stockMovement")}>
              {t("New movement")}
            </Button>
          </div>
        )}
        <div className="segmented" role="group" aria-label={t("Stock view")}>
          <button type="button" aria-pressed={view === "positions"} onClick={() => setView("positions")}>
            {t("Position")}
          </button>
          <button type="button" aria-pressed={view === "movements"} onClick={() => setView("movements")}>
            {t("Movements")}
          </button>
          <button type="button" aria-pressed={view === "alerts"} onClick={() => setView("alerts")}>
            {t("Below minimum")}
            {alerts !== null && alerts.length > 0 && ` (${alerts.length})`}
          </button>
          <button type="button" aria-pressed={view === "settings"} onClick={() => setView("settings")}>
            {t("Settings")}
          </button>
          {canWrite && (
            <button type="button" aria-pressed={view === "import"} onClick={() => setView("import")}>
              {t("Import balances")}
            </button>
          )}
        </div>
        <Notice message={productsError} />
        {view === "positions" ? (
          <PositionsView companyId={companyId} lowIds={new Set((alerts ?? []).map((alert) => alert.product_id))} />
        ) : view === "movements" ? (
          <MovementsView companyId={companyId} products={products} canWrite={canWrite} />
        ) : view === "alerts" ? (
          <AlertsView alerts={alerts} error={alertsError} />
        ) : view === "import" ? (
          <ImportView companyId={companyId} onImported={loadAlerts} />
        ) : (
          <SettingsView companyId={companyId} canWrite={canWrite} />
        )}
    </>
  );
}

type PositionColumn = "minimum" | "cost" | "value";
const POSITION_COLUMNS: ColumnDef<PositionColumn>[] = [
  { key: "minimum", label: msg("Minimum stock"), hiddenByDefault: true },
  { key: "cost", label: msg("Average cost") },
  { key: "value", label: msg("Value") },
];

function PositionsView({ companyId, lowIds }: { companyId: string; lowIds: Set<string> }) {
  const t = useT();
  const [filters, setFilters] = useState<StockPositionFilters>(NO_POSITION_FILTERS);
  const { items, nextCursor, loading, loadingMore, error, applied, reload, loadMore } = usePagedList<StockPosition, StockPositionFilters>(
    (applied, after) => api.listStockPositions(companyId, { ...applied, on: wholeDate(applied.on) }, after),
    filters,
  );
  const columns = useColumns("stock-position", POSITION_COLUMNS);
  const columnDefs = POSITION_COLUMNS.map((column) => ({ ...column, label: t(column.label) }));
  useOnCreated("stockMovement", () => void reload());
  const isFiltered = applied.q !== "" || applied.on !== "" || applied.situation !== "";
  const summary = useCounts(
    () => api.getStockPositionSummary(companyId, { q: applied.q, on: wholeDate(applied.on) }),
    items,
    !loading,
  );
  const tiles: TileOption<StockPositionFilters["situation"]>[] = [
    { key: "", label: t("Every product"), count: summary?.total ?? null },
    { key: "in_stock", label: t("In stock"), count: summary?.in_stock ?? null },
    { key: "below_minimum", label: t("Below minimum"), count: summary?.below_minimum ?? null },
    { key: "depleted", label: t("Depleted"), count: summary?.depleted ?? null },
  ];

  return (
    <>
      <div className="list-actions">
        <ColumnChooser columns={columnDefs} isShown={columns.isShown} onToggle={columns.toggle} onReset={columns.reset} />
        <ListTools
          filename={t("stock-position")}
          header={[t("Product"), "SKU", t("Unit"), t("Quantity"), t("Average cost"), t("Value")]}
          fetchPage={(after) => api.listStockPositions(companyId, { ...applied, on: wholeDate(applied.on) }, after)}
          toRow={(position) => [
            position.name,
            position.sku ?? "",
            position.unit,
            toInputDecimal(position.quantity),
            position.average_cost === null ? "" : toInputDecimal(trimCost(position.average_cost)),
            toInputDecimal(position.value),
          ]}
        />
      </div>
      <section className="card list-card">
        <h2>{t("Stock position")}</h2>
        <FilterTiles
          label={t("Status")}
          value={filters.situation}
          options={tiles}
          onChange={(situation) => setFilters({ ...filters, situation })}
        />
        <div className="filters" role="search">
          <label>
            {t("Search")}
            <input
              type="search"
              value={filters.q}
              placeholder={t("Name or code")}
              autoComplete="off"
              maxLength={100}
              onChange={(event) => setFilters({ ...filters, q: event.target.value })}
            />
          </label>
          <label>
            {t("As of")}
            <input type="date" value={filters.on} onChange={(event) => setFilters({ ...filters, on: event.target.value })} />
          </label>
        </div>
        <Notice message={error} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        ) : items.length === 0 && error ? (
          <Button type="button" variant="ghost" onClick={() => void reload()}>
            {t("Try again")}
          </Button>
        ) : items.length === 0 ? (
          <div className="empty">
            <BoxIcon size={40} aria-hidden />
            <p role={isFiltered ? "status" : undefined}>{isFiltered ? t("No products found.") : t("No products in stock yet.")}</p>
            {isFiltered ? (
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_POSITION_FILTERS })}>
                {t("Clear filters")}
              </Button>
            ) : (
              <p className="muted">{t("Stock comes from received purchases, sales and the movements entered here.")}</p>
            )}
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Product")}</th>
                  <th className="num">{t("Quantity")}</th>
                  {columns.isShown("minimum") && <th className="num">{t("Minimum")}</th>}
                  {columns.isShown("cost") && <th className="num">{t("Average cost")}</th>}
                  {columns.isShown("value") && <th className="num">{t("Value")}</th>}
                </tr>
              </thead>
              <tbody>
                {items.map((position, index) => (
                  <ListRow key={position.product_id} index={index} highlighted={false}>
                    <td>
                      <div className="cell-title">{position.name}</div>
                      {position.sku !== null && <div className="muted">{position.sku}</div>}
                    </td>
                    <td className="num">
                      {formatDecimal(position.quantity)} {position.unit}
                      {isNegative(position.quantity) ? (
                        <div>
                          <span className="badge badge-dot badge-critical">{t("Negative balance")}</span>
                        </div>
                      ) : (
                        lowIds.has(position.product_id) && (
                          <div>
                            <span className="badge badge-dot badge-warning">{t("Below minimum")}</span>
                          </div>
                        )
                      )}
                    </td>
                    {columns.isShown("minimum") && (
                      <td className="num">{position.minimum_stock === null ? "—" : `${formatDecimal(position.minimum_stock)} ${position.unit}`}</td>
                    )}
                    {columns.isShown("cost") && (
                      <td className="num">{position.average_cost === null ? "—" : formatMoney(trimCost(position.average_cost))}</td>
                    )}
                    {columns.isShown("value") && <td className="num">{formatMoney(position.value)}</td>}
                  </ListRow>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {summary !== null && summary.total > 0 && (
          <p className="muted list-summary" role="status">
            {summary.total === 1
              ? t("Total: {count} product · Total cost {value}", { count: summary.total, value: formatMoney(summary.value) })
              : t("Total: {count} products · Total cost {value}", { count: summary.total, value: formatMoney(summary.value) })}
          </p>
        )}
        {nextCursor !== null && (
          <Button type="button" variant="ghost" icon={ChevronDownIcon} pending={loadingMore} onClick={() => void loadMore()}>
            {t("Load more")}
          </Button>
        )}
      </section>
    </>
  );
}

function MovementsView({ companyId, products, canWrite }: { companyId: string; products: Product[]; canWrite: boolean }) {
  const t = useT();
  const [filters, setFilters] = useState<StockMovementFilters>(NO_MOVEMENT_FILTERS);
  const [reversing, setReversing] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const { items, nextCursor, loading, loadingMore, error, applied, reload, loadMore } = usePagedList<StockMovement, StockMovementFilters>(
    (applied, after) => api.listStockMovements(companyId, { ...applied, from: wholeDate(applied.from), to: wholeDate(applied.to) }, after),
    filters,
  );
  useOnCreated("stockMovement", () => void reload());
  const isFiltered = applied.product !== "" || applied.kind !== "" || applied.from !== "" || applied.to !== "";
  const counts = useCounts(
    () => api.getStockKindCounts(companyId, { product: applied.product, from: wholeDate(applied.from), to: wholeDate(applied.to) }),
    items,
    !loading,
  );
  const tiles: TileOption<StockMovementFilters["kind"]>[] = [
    { key: "", label: t("All"), count: counts === null ? null : counts.in + counts.out + counts.adjustment },
    { key: "in", label: t("Stock entries"), count: counts?.in ?? null },
    { key: "out", label: t("Exits"), count: counts?.out ?? null },
    { key: "adjustment", label: t("Adjustments"), count: counts?.adjustment ?? null },
  ];
  const names = useMemo(() => new Map(products.map((product) => [product.id, product.name])), [products]);
  // What the loaded rows already undo: another page may hold an undoing, and then the API says so.
  const undone = useMemo(() => new Set(items.flatMap((movement) => (movement.reverses === null ? [] : [movement.reverses]))), [items]);

  return (
    <>
      <div className="list-actions">
        <ListTools
          filename={t("stock-movements")}
          header={[t("Date"), t("Product"), t("Type"), t("Quantity"), t("Value"), t("Origin"), t("Note")]}
          fetchPage={(after) => api.listStockMovements(companyId, { ...applied, from: wholeDate(applied.from), to: wholeDate(applied.to) }, after)}
          toRow={(movement) => [
            movement.occurred_on,
            names.get(movement.product_id) ?? "",
            t(KIND_LABELS[movement.kind]),
            toInputDecimal(movement.quantity),
            toInputDecimal(movement.value),
            movement.reverses !== null ? t("Undoes a movement") : movement.origin !== null ? t(ORIGIN_LABELS[movement.origin.kind]) : t("Manual"),
            movement.note ?? "",
          ]}
        />
      </div>
      <section className="card list-card">
        <h2>{t("Movements")}</h2>
        <FilterTiles label={t("Type")} value={filters.kind} options={tiles} onChange={(kind) => setFilters({ ...filters, kind })} />
        <div className="filters" role="search">
          <label>
            {t("Product")}
            <select value={filters.product} onChange={(event) => setFilters({ ...filters, product: event.target.value })}>
              <option value="">{t("Every product")}</option>
              {products.map((product) => (
                <option key={product.id} value={product.id}>
                  {product.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("From")}
            <input type="date" value={filters.from} onChange={(event) => setFilters({ ...filters, from: event.target.value })} />
          </label>
          <label>
            {t("To")}
            <input type="date" value={filters.to} onChange={(event) => setFilters({ ...filters, to: event.target.value })} />
          </label>
        </div>
        <Notice message={error} />
        <Notice message={saved} tone="success" />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        ) : items.length === 0 && error ? (
          <Button type="button" variant="ghost" onClick={() => void reload()}>
            {t("Try again")}
          </Button>
        ) : items.length === 0 ? (
          <div className="empty">
            <BoxIcon size={40} aria-hidden />
            <p role={isFiltered ? "status" : undefined}>{isFiltered ? t("No movements found.") : t("No movements yet.")}</p>
            {isFiltered && (
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_MOVEMENT_FILTERS })}>
                {t("Clear filters")}
              </Button>
            )}
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Date")}</th>
                  <th>{t("Product")}</th>
                  <th>{t("Type")}</th>
                  <th className="num">{t("Quantity")}</th>
                  <th className="num">{t("Value")}</th>
                  <th>{t("Origin")}</th>
                  <th>
                    <span className="visually-hidden">{t("Actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {items.map((movement, index) => {
                  const name = names.get(movement.product_id) ?? "";
                  const manual = movement.origin === null && movement.reverses === null;
                  return (
                    <Fragment key={movement.id}>
                      <ListRow index={index} highlighted={false}>
                        <td>{formatDate(movement.occurred_on)}</td>
                        <td>
                          <div className="cell-title">{name}</div>
                          {movement.note !== null && <div className="muted">{movement.note}</div>}
                        </td>
                        <td>{t(KIND_LABELS[movement.kind])}</td>
                        <td className="num">{signedQuantity(formatDecimal(movement.quantity))}</td>
                        <td className="num">{formatMoney(movement.value)}</td>
                        <td>
                          {movement.reverses !== null
                            ? t("Undoes a movement")
                            : movement.origin !== null
                              ? t(ORIGIN_LABELS[movement.origin.kind])
                              : t("Manual")}
                          {undone.has(movement.id) && (
                            <>
                              {" "}
                              <span className="badge badge-dot badge-warning">{t("Undone")}</span>
                            </>
                          )}
                        </td>
                        <td className="row-actions">
                          {canWrite && manual && !undone.has(movement.id) && (
                            <Button
                              type="button"
                              variant="ghost"
                              icon={RotateCCWIcon}
                              className="btn-small"
                              aria-label={t("Undo the movement of {date} for {name}", { date: formatDate(movement.occurred_on), name })}
                              aria-expanded={reversing === movement.id}
                              onClick={() => setReversing(reversing === movement.id ? null : movement.id)}
                            >
                              {t("Undo")}
                            </Button>
                          )}
                        </td>
                      </ListRow>
                      {reversing === movement.id && (
                        <tr>
                          <td colSpan={7} role="region" aria-label={t("Undo the movement of {date} for {name}", { date: formatDate(movement.occurred_on), name })}>
                            <ReversePanel
                              companyId={companyId}
                              movement={movement}
                              onDone={() => {
                                setReversing(null);
                                setSaved(t("Movement undone: an opposite movement was entered."));
                                void reload();
                              }}
                            />
                          </td>
                        </tr>
                      )}
                    </Fragment>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
        {nextCursor !== null && (
          <Button type="button" variant="ghost" icon={ChevronDownIcon} pending={loadingMore} onClick={() => void loadMore()}>
            {t("Load more")}
          </Button>
        )}
      </section>
    </>
  );
}

/** Undoing a movement writes another with the opposite quantity and value, on the day chosen here. */
function ReversePanel({ companyId, movement, onDone }: { companyId: string; movement: StockMovement; onDone: () => void }) {
  const t = useT();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (!isIsoDate(field("occurred_on"))) {
      setError(t("Enter the day the movement is undone."));
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await api.reverseStockMovement(companyId, movement.id, { occurred_on: field("occurred_on"), note: field("note") || undefined });
      onDone();
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  return (
    <form className="settle-form" onSubmit={(event) => void submit(event)}>
      <p>
        {t("Undoing enters a movement opposite to the one of {date}; the original stays in the ledger.", { date: formatDate(movement.occurred_on) })}
      </p>
      <label>
        {t("Reversal date")}
        <input name="occurred_on" type="date" required defaultValue={todayIso()} />
      </label>
      <label>
        {t("Note")}
        <input name="note" maxLength={200} autoComplete="off" />
      </label>
      <Notice message={error} />
      <Button type="submit" pending={saving}>
        {t("Undo movement")}
      </Button>
    </form>
  );
}

/** The products that have less than their minimum, as the server worked them out. */
function AlertsView({ alerts, error }: { alerts: StockAlert[] | null; error: string | null }) {
  const t = useT();
  return (
    <section className="card list-card">
      <h2>{t("Below minimum")}</h2>
      <Notice message={error} />
      {alerts === null ? (
        error === null && (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        )
      ) : alerts.length === 0 ? (
        <div className="empty">
          <BoxIcon size={40} aria-hidden />
          <p>{t("No product below the minimum.")}</p>
          <p className="muted">{t("Each product's minimum is set in the product register.")}</p>
        </div>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>{t("Product")}</th>
                <th className="num">{t("In stock")}</th>
                <th className="num">{t("Minimum")}</th>
                <th className="num">{t("Shortfall")}</th>
              </tr>
            </thead>
            <tbody>
              {alerts.map((alert, index) => (
                <ListRow key={alert.product_id} index={index} highlighted={false}>
                  <td>
                    <div className="cell-title">{alert.name}</div>
                    {alert.sku !== null && <div className="muted">{alert.sku}</div>}
                  </td>
                  <td className="num">
                    {formatDecimal(alert.quantity)} {alert.unit}
                  </td>
                  <td className="num">
                    {formatDecimal(alert.minimum_stock)} {alert.unit}
                  </td>
                  <td className="num">
                    {formatDecimal(alert.shortfall)} {alert.unit}
                  </td>
                </ListRow>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

const SETTING_LABELS: Record<string, string> = { block_sale_without_balance: msg("Block sales without balance") };

const settingValue = (t: Translate, field: string, value: string | null) =>
  field === "block_sale_without_balance" ? (value === "true" ? t("Switched on") : t("Switched off")) : (value ?? "—");

const STOCK_TEMPLATE = ["sku,quantity,unit_cost", "CAFE-500,10,5.00", "PAO-1,3,"].join("\n");

/** Opens the stock of products from a file, once: the name of the file and the day are what sending it again recognises. */
function ImportView({ companyId, onImported }: { companyId: string; onImported: () => void }) {
  const { t, rich } = useI18n();
  const [reference, setReference] = useState("");
  const [on, setOn] = useState(todayIso());
  const ready = reference.trim() !== "" && isIsoDate(on);
  return (
    <>
      <section className="card">
        <div className="card-title">
          <h2>{t("Opening balance")}</h2>
        </div>
        <div className="form-grid">
          <label>
            {t("Name of this import")}
            <input value={reference} maxLength={100} onChange={(event) => setReference(event.target.value)} />
            <span className="field-hint">
              {t("Sending a file with the same name again adds nothing. Use a new name for each file, such as “migration 2026”.")}
            </span>
          </label>
          <label>
            {t("Balance day")}
            <input type="date" value={on} onChange={(event) => setOn(event.target.value)} />
          </label>
        </div>
      </section>
      <CsvImport
        title={t("Import balances from a CSV file")}
        template={STOCK_TEMPLATE}
        templateName={t("stock-balances.csv")}
        created={[t("opening balance"), t("opening balances")]}
        blocked={!ready}
        run={(file, dryRun) => api.importStockBalances(companyId, file, reference.trim(), on, dryRun)}
        onImported={onImported}
      >
        <p>
          {rich(
            "The first row names the columns, in any order; a comma or a semicolon separates them. {sku} and {quantity} are required; {unit_cost} is the cost of one unit (blank, the product's cost price applies). It only opens the balance of a product that has no movements yet: to correct an existing balance, make an inventory count. A quantity of zero opens nothing.",
            { sku: <code>sku</code>, quantity: <code>quantity</code>, unit_cost: <code>unit_cost</code> },
          )}
        </p>
      </CsvImport>
    </>
  );
}

/** The company's stock rules: today, whether a sale the stock cannot cover is refused. */
function SettingsView({ companyId, canWrite }: { companyId: string; canWrite: boolean }) {
  const t = useT();
  const [settings, setSettings] = useState<StockSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // Bumped by a save so the trail below reads again.
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let current = true;
    api
      .getStockSettings(companyId)
      .then((stored) => current && setSettings(stored))
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  async function change(block: boolean) {
    if (saving) return;
    setSaving(true);
    setError(null);
    setSaved(null);
    try {
      setSettings(await api.putStockSettings(companyId, { block_sale_without_balance: block }));
      setVersion((current) => current + 1);
      setSaved(t("Setting saved."));
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  // eslint-disable-next-line react-hooks/exhaustive-deps -- `version` only makes the trail read again after a save
  const historyLoader = useCallback(() => api.stockSettingsHistory(companyId), [companyId, version]);

  return (
    <section className="card">
      <h2>{t("Stock settings")}</h2>
      <Notice message={error} />
      <Notice message={saved} tone="success" />
      {settings === null ? (
        error === null && (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        )
      ) : (
        <>
          <label className="inline">
            <input
              type="checkbox"
              role="switch"
              checked={settings.block_sale_without_balance}
              disabled={!canWrite || saving}
              onChange={(event) => void change(event.target.checked)}
            />{" "}
            {t("Block sales without balance")}
          </label>
          <p className="muted">
            {t("When on, finishing a sale the stock cannot cover is refused: the order stays open and nothing is entered. When off, the sale goes through and the product's balance may go negative.")}
          </p>
          <h3>{t("History")}</h3>
          <History
            load={historyLoader}
            fieldLabel={(field) => {
              const label = SETTING_LABELS[field];
              return label === undefined ? field : t(label);
            }}
            valueLabel={(field, value) => settingValue(t, field, value)}
            emptyMessage={t("No records.")}
          />
        </>
      )}
    </section>
  );
}
