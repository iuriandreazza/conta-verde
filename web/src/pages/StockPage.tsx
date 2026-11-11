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
  in: "Entrada",
  out: "Saída",
  adjustment: "Ajuste",
};

const ORIGIN_LABELS: Record<StockOriginKind, string> = {
  purchase_receipt: "Compra",
  sale: "Venda",
  sale_return: "Devolução",
  inventory_count: "Inventário",
  import: "Saldo inicial",
};

/** A date field that is not a whole date yet (a year still being typed) asks nothing of the API. */

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function StockPage() {
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
          title="Estoque"
          description="O que há de cada produto, a que custo médio, e o livro de movimentações: compras, vendas e devoluções entram sozinhas; ajustes e entradas avulsas são lançados aqui."
        />
        {canWrite && (
          <div className="list-actions">
            <Button type="button" icon={PlusIcon} onClick={() => openCreate("stockMovement")}>
              Novo movimento
            </Button>
          </div>
        )}
        <div className="segmented" role="group" aria-label="Visão do estoque">
          <button type="button" aria-pressed={view === "positions"} onClick={() => setView("positions")}>
            Posição
          </button>
          <button type="button" aria-pressed={view === "movements"} onClick={() => setView("movements")}>
            Movimentações
          </button>
          <button type="button" aria-pressed={view === "alerts"} onClick={() => setView("alerts")}>
            Abaixo do mínimo{alerts !== null && alerts.length > 0 && ` (${alerts.length})`}
          </button>
          <button type="button" aria-pressed={view === "settings"} onClick={() => setView("settings")}>
            Configurações
          </button>
          {canWrite && (
            <button type="button" aria-pressed={view === "import"} onClick={() => setView("import")}>
              Importar saldos
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
  { key: "minimum", label: "Estoque mínimo", hiddenByDefault: true },
  { key: "cost", label: "Custo médio" },
  { key: "value", label: "Valor" },
];

function PositionsView({ companyId, lowIds }: { companyId: string; lowIds: Set<string> }) {
  const [filters, setFilters] = useState<StockPositionFilters>(NO_POSITION_FILTERS);
  const { items, nextCursor, loading, loadingMore, error, applied, reload, loadMore } = usePagedList<StockPosition, StockPositionFilters>(
    (applied, after) => api.listStockPositions(companyId, { ...applied, on: wholeDate(applied.on) }, after),
    filters,
  );
  const columns = useColumns("stock-position", POSITION_COLUMNS);
  useOnCreated("stockMovement", () => void reload());
  const isFiltered = applied.q !== "" || applied.on !== "" || applied.situation !== "";
  const summary = useCounts(
    () => api.getStockPositionSummary(companyId, { q: applied.q, on: wholeDate(applied.on) }),
    items,
    !loading,
  );
  const tiles: TileOption<StockPositionFilters["situation"]>[] = [
    { key: "", label: "Todos", count: summary?.total ?? null },
    { key: "in_stock", label: "Em estoque", count: summary?.in_stock ?? null },
    { key: "below_minimum", label: "Abaixo do mínimo", count: summary?.below_minimum ?? null },
    { key: "depleted", label: "Esgotado", count: summary?.depleted ?? null },
  ];

  return (
    <>
      <div className="list-actions">
        <ColumnChooser columns={POSITION_COLUMNS} isShown={columns.isShown} onToggle={columns.toggle} onReset={columns.reset} />
        <ListTools
          filename="posicao-do-estoque"
          header={["Produto", "SKU", "Unidade", "Quantidade", "Custo médio", "Valor"]}
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
        <h2>Posição do estoque</h2>
        <FilterTiles
          label="Situação"
          value={filters.situation}
          options={tiles}
          onChange={(situation) => setFilters({ ...filters, situation })}
        />
        <div className="filters" role="search">
          <label>
            Buscar
            <input
              type="search"
              value={filters.q}
              placeholder="Nome ou código"
              autoComplete="off"
              maxLength={100}
              onChange={(event) => setFilters({ ...filters, q: event.target.value })}
            />
          </label>
          <label>
            Em
            <input type="date" value={filters.on} onChange={(event) => setFilters({ ...filters, on: event.target.value })} />
          </label>
        </div>
        <Notice message={error} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        ) : items.length === 0 && error ? (
          <Button type="button" variant="ghost" onClick={() => void reload()}>
            Tentar de novo
          </Button>
        ) : items.length === 0 ? (
          <div className="empty">
            <BoxIcon size={40} aria-hidden />
            <p role={isFiltered ? "status" : undefined}>{isFiltered ? "Nenhum produto encontrado." : "Nenhum produto em estoque ainda."}</p>
            {isFiltered ? (
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_POSITION_FILTERS })}>
                Limpar filtros
              </Button>
            ) : (
              <p className="muted">O estoque nasce das compras recebidas, das vendas e dos movimentos lançados aqui.</p>
            )}
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Produto</th>
                  <th className="num">Quantidade</th>
                  {columns.isShown("minimum") && <th className="num">Mínimo</th>}
                  {columns.isShown("cost") && <th className="num">Custo médio</th>}
                  {columns.isShown("value") && <th className="num">Valor</th>}
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
                          <span className="badge badge-dot badge-critical">Saldo negativo</span>
                        </div>
                      ) : (
                        lowIds.has(position.product_id) && (
                          <div>
                            <span className="badge badge-dot badge-warning">Abaixo do mínimo</span>
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
            Total: {summary.total} {summary.total === 1 ? "produto" : "produtos"} · Custo total {formatMoney(summary.value)}
          </p>
        )}
        {nextCursor !== null && (
          <Button type="button" variant="ghost" icon={ChevronDownIcon} pending={loadingMore} onClick={() => void loadMore()}>
            Carregar mais
          </Button>
        )}
      </section>
    </>
  );
}

function MovementsView({ companyId, products, canWrite }: { companyId: string; products: Product[]; canWrite: boolean }) {
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
    { key: "", label: "Todas", count: counts === null ? null : counts.in + counts.out + counts.adjustment },
    { key: "in", label: "Entradas", count: counts?.in ?? null },
    { key: "out", label: "Saídas", count: counts?.out ?? null },
    { key: "adjustment", label: "Ajustes", count: counts?.adjustment ?? null },
  ];
  const names = useMemo(() => new Map(products.map((product) => [product.id, product.name])), [products]);
  // What the loaded rows already undo: another page may hold an undoing, and then the API says so.
  const undone = useMemo(() => new Set(items.flatMap((movement) => (movement.reverses === null ? [] : [movement.reverses]))), [items]);

  return (
    <>
      <div className="list-actions">
        <ListTools
          filename="movimentacoes-de-estoque"
          header={["Data", "Produto", "Tipo", "Quantidade", "Valor", "Origem", "Observação"]}
          fetchPage={(after) => api.listStockMovements(companyId, { ...applied, from: wholeDate(applied.from), to: wholeDate(applied.to) }, after)}
          toRow={(movement) => [
            movement.occurred_on,
            names.get(movement.product_id) ?? "",
            KIND_LABELS[movement.kind],
            toInputDecimal(movement.quantity),
            toInputDecimal(movement.value),
            movement.reverses !== null ? "Desfaz movimento" : movement.origin !== null ? ORIGIN_LABELS[movement.origin.kind] : "Manual",
            movement.note ?? "",
          ]}
        />
      </div>
      <section className="card list-card">
        <h2>Movimentações</h2>
        <FilterTiles label="Tipo" value={filters.kind} options={tiles} onChange={(kind) => setFilters({ ...filters, kind })} />
        <div className="filters" role="search">
          <label>
            Produto
            <select value={filters.product} onChange={(event) => setFilters({ ...filters, product: event.target.value })}>
              <option value="">Todos</option>
              {products.map((product) => (
                <option key={product.id} value={product.id}>
                  {product.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            De
            <input type="date" value={filters.from} onChange={(event) => setFilters({ ...filters, from: event.target.value })} />
          </label>
          <label>
            Até
            <input type="date" value={filters.to} onChange={(event) => setFilters({ ...filters, to: event.target.value })} />
          </label>
        </div>
        <Notice message={error} />
        <Notice message={saved} tone="success" />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        ) : items.length === 0 && error ? (
          <Button type="button" variant="ghost" onClick={() => void reload()}>
            Tentar de novo
          </Button>
        ) : items.length === 0 ? (
          <div className="empty">
            <BoxIcon size={40} aria-hidden />
            <p role={isFiltered ? "status" : undefined}>{isFiltered ? "Nenhuma movimentação encontrada." : "Nenhuma movimentação ainda."}</p>
            {isFiltered && (
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_MOVEMENT_FILTERS })}>
                Limpar filtros
              </Button>
            )}
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Data</th>
                  <th>Produto</th>
                  <th>Tipo</th>
                  <th className="num">Quantidade</th>
                  <th className="num">Valor</th>
                  <th>Origem</th>
                  <th>
                    <span className="visually-hidden">Ações</span>
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
                        <td>{KIND_LABELS[movement.kind]}</td>
                        <td className="num">{signedQuantity(formatDecimal(movement.quantity))}</td>
                        <td className="num">{formatMoney(movement.value)}</td>
                        <td>
                          {movement.reverses !== null
                            ? "Desfaz movimento"
                            : movement.origin !== null
                              ? ORIGIN_LABELS[movement.origin.kind]
                              : "Manual"}
                          {undone.has(movement.id) && (
                            <>
                              {" "}
                              <span className="badge badge-dot badge-warning">Desfeito</span>
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
                              aria-label={`Desfazer o movimento de ${formatDate(movement.occurred_on)} de ${name}`}
                              aria-expanded={reversing === movement.id}
                              onClick={() => setReversing(reversing === movement.id ? null : movement.id)}
                            >
                              Desfazer
                            </Button>
                          )}
                        </td>
                      </ListRow>
                      {reversing === movement.id && (
                        <tr>
                          <td colSpan={7} role="region" aria-label={`Desfazer o movimento de ${formatDate(movement.occurred_on)} de ${name}`}>
                            <ReversePanel
                              companyId={companyId}
                              movement={movement}
                              onDone={() => {
                                setReversing(null);
                                setSaved("Movimento desfeito: um movimento contrário foi lançado.");
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
            Carregar mais
          </Button>
        )}
      </section>
    </>
  );
}

/** Undoing a movement writes another with the opposite quantity and value, on the day chosen here. */
function ReversePanel({ companyId, movement, onDone }: { companyId: string; movement: StockMovement; onDone: () => void }) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (!isIsoDate(field("occurred_on"))) {
      setError("Informe o dia em que o movimento é desfeito.");
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
      <p>Desfazer lança um movimento contrário ao de {formatDate(movement.occurred_on)}; o original continua no livro.</p>
      <label>
        Dia do desfazimento
        <input name="occurred_on" type="date" required defaultValue={todayIso()} />
      </label>
      <label>
        Observação
        <input name="note" maxLength={200} autoComplete="off" />
      </label>
      <Notice message={error} />
      <Button type="submit" pending={saving}>
        Desfazer movimento
      </Button>
    </form>
  );
}

/** The products that have less than their minimum, as the server worked them out. */
function AlertsView({ alerts, error }: { alerts: StockAlert[] | null; error: string | null }) {
  return (
    <section className="card list-card">
      <h2>Abaixo do mínimo</h2>
      <Notice message={error} />
      {alerts === null ? (
        error === null && (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        )
      ) : alerts.length === 0 ? (
        <div className="empty">
          <BoxIcon size={40} aria-hidden />
          <p>Nenhum produto abaixo do mínimo.</p>
          <p className="muted">O mínimo de cada produto é definido no cadastro de produtos.</p>
        </div>
      ) : (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Produto</th>
                <th className="num">Em estoque</th>
                <th className="num">Mínimo</th>
                <th className="num">Falta</th>
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

const SETTING_LABELS: Record<string, string> = { block_sale_without_balance: "Bloquear venda sem saldo" };

const settingValue = (field: string, value: string | null) =>
  field === "block_sale_without_balance" ? (value === "true" ? "Ligado" : "Desligado") : (value ?? "—");

/** The company's stock rules: today, whether a sale the stock cannot cover is refused. */
const STOCK_TEMPLATE = ["sku,quantity,unit_cost", "CAFE-500,10,5.00", "PAO-1,3,"].join("\n");

/** Opens the stock of products from a file, once: the name of the file and the day are what sending it again recognises. */
function ImportView({ companyId, onImported }: { companyId: string; onImported: () => void }) {
  const [reference, setReference] = useState("");
  const [on, setOn] = useState(todayIso());
  const ready = reference.trim() !== "" && isIsoDate(on);
  return (
    <>
      <section className="card">
        <div className="card-title">
          <h2>Saldo inicial</h2>
        </div>
        <div className="form-grid">
          <label>
            Nome desta importação
            <input value={reference} maxLength={100} onChange={(event) => setReference(event.target.value)} />
            <span className="field-hint">
              Enviar de novo um arquivo com o mesmo nome não soma nada. Use um nome novo para cada arquivo, como “migração 2026”.
            </span>
          </label>
          <label>
            Dia do saldo
            <input type="date" value={on} onChange={(event) => setOn(event.target.value)} />
          </label>
        </div>
      </section>
      <CsvImport
        title="Importar saldos de um arquivo CSV"
        template={STOCK_TEMPLATE}
        templateName="saldos-de-estoque.csv"
        created={["saldo aberto", "saldos abertos"]}
        blocked={!ready}
        run={(file, dryRun) => api.importStockBalances(companyId, file, reference.trim(), on, dryRun)}
        onImported={onImported}
      >
        <p>
          A primeira linha dá o nome das colunas, em qualquer ordem; a vírgula ou o ponto e vírgula separam.{" "}
          <code>sku</code> e <code>quantity</code> são obrigatórias; <code>unit_cost</code> é o custo de uma unidade (em
          branco, vale o preço de custo do produto). Só abre o saldo de um produto que ainda não tem movimentos: para
          corrigir um saldo existente, faça um inventário. Quantidade zero não abre nada.
        </p>
      </CsvImport>
    </>
  );
}

function SettingsView({ companyId, canWrite }: { companyId: string; canWrite: boolean }) {
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
      setSaved("Configuração salva.");
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
      <h2>Configurações do estoque</h2>
      <Notice message={error} />
      <Notice message={saved} tone="success" />
      {settings === null ? (
        error === null && (
          <p role="status" className="status">
            <Spinner /> Carregando…
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
            Bloquear venda sem saldo
          </label>
          <p className="muted">
            Ligado, concluir uma venda que o estoque não cobre é recusado: o pedido continua aberto e nada é lançado.
            Desligado, a venda sai e o saldo do produto pode ficar negativo.
          </p>
          <h3>Histórico</h3>
          <History
            load={historyLoader}
            fieldLabel={(field) => SETTING_LABELS[field] ?? field}
            valueLabel={settingValue}
            emptyMessage="Sem registros."
          />
        </>
      )}
    </section>
  );
}
