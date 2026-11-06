import { Fragment, useMemo, useState, type FormEvent } from "react";
import {
  NO_MOVEMENT_FILTERS,
  NO_POSITION_FILTERS,
  api,
  errorMessage,
  type Product,
  type StockMovement,
  type StockMovementFilters,
  type StockMovementKind,
  type StockOriginKind,
  type StockPosition,
  type StockPositionFilters,
} from "../api";
import { Button } from "../components/Button";
import { BoxIcon } from "../components/icons/box";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { PlusIcon } from "../components/icons/plus";
import { RotateCCWIcon } from "../components/icons/rotate-ccw";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { formatDecimal, formatMoney } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { isBelowMinimum, isNegative, signedQuantity, trimCost } from "../lib/stock";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";
import { useStockProducts } from "../lib/useStockProducts";
import { useParams } from "react-router-dom";

type View = "positions" | "movements";

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
};

/** A date field that is not a whole date yet (a year still being typed) asks nothing of the API. */
const wholeDate = (day: string) => (isIsoDate(day) && day >= "1000-01-01" ? day : "");

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function StockPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("purchases");
  const { openCreate } = useCreation();
  const [view, setView] = useState<View>("positions");
  const { products, error: productsError } = useStockProducts(companyId);

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
      </div>
      <Notice message={productsError} />
      {view === "positions" ? (
        <PositionsView companyId={companyId} />
      ) : (
        <MovementsView companyId={companyId} products={products} canWrite={canWrite} />
      )}
    </>
  );
}

function PositionsView({ companyId }: { companyId: string }) {
  const [filters, setFilters] = useState<StockPositionFilters>(NO_POSITION_FILTERS);
  const { items, nextCursor, loading, loadingMore, error, applied, reload, loadMore } = usePagedList<StockPosition, StockPositionFilters>(
    (applied, after) => api.listStockPositions(companyId, { ...applied, on: wholeDate(applied.on) }, after),
    filters,
  );
  useOnCreated("stockMovement", () => void reload());
  const isFiltered = applied.q !== "" || applied.on !== "";

  return (
    <section className="card list-card">
      <h2>Posição do estoque</h2>
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
                <th className="num">Custo médio</th>
                <th className="num">Valor</th>
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
                      isBelowMinimum(position.quantity, position.minimum_stock) && (
                        <div>
                          <span className="badge badge-dot badge-warning">Abaixo do mínimo</span>
                        </div>
                      )
                    )}
                  </td>
                  <td className="num">{position.average_cost === null ? "—" : formatMoney(trimCost(position.average_cost))}</td>
                  <td className="num">{formatMoney(position.value)}</td>
                </ListRow>
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
  const names = useMemo(() => new Map(products.map((product) => [product.id, product.name])), [products]);
  // What the loaded rows already undo: another page may hold an undoing, and then the API says so.
  const undone = useMemo(() => new Set(items.flatMap((movement) => (movement.reverses === null ? [] : [movement.reverses]))), [items]);

  return (
    <section className="card list-card">
      <h2>Movimentações</h2>
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
          Tipo
          <select value={filters.kind} onChange={(event) => setFilters({ ...filters, kind: event.target.value as StockMovementFilters["kind"] })}>
            <option value="">Todos</option>
            {Object.entries(KIND_LABELS).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
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
