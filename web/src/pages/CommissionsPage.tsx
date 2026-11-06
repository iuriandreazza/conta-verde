import { Fragment, useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import {
  NO_COMMISSION_RULE_FILTERS,
  NO_FILTERS,
  NO_PRODUCT_FILTERS,
  api,
  errorMessage,
  type CommissionRule,
  type CommissionRuleFilters,
  type CommissionRuleInput,
  type CommissionStatement,
  type Party,
  type Product,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HandCoinsIcon } from "../components/icons/hand-coins";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { SquarePenIcon } from "../components/icons/square-pen";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { StatusBadge } from "../components/StatusBadge";
import { formatDecimal, formatMoney, isAmbiguousDecimal, toApiDecimal, toInputDecimal } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { todayIso } from "../lib/settlement";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

const FIELD_LABELS: Record<string, string> = {
  seller_id: "Vendedor",
  product_id: "Produto",
  rate: "Percentual",
  starts_on: "Vale desde",
  ends_on: "Vale até",
  active: "Situação",
};

const rate = (value: string) => `${formatDecimal(value)}%`;

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function CommissionsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("sales");
  const [parties, setParties] = useState<Party[]>([]);
  const [products, setProducts] = useState<Product[]>([]);
  const [targetsError, setTargetsError] = useState<string | null>(null);
  const [targetsLoaded, setTargetsLoaded] = useState(false);

  useEffect(() => {
    let current = true;
    Promise.all([
      readUpTo((after) => api.listParties(companyId, NO_FILTERS, after)),
      readUpTo((after) => api.listProducts(companyId, NO_PRODUCT_FILTERS, after)),
    ])
      .then(([partyList, productList]) => {
        if (current) {
          setParties(partyList);
          setProducts(productList);
          setTargetsLoaded(true);
        }
      })
      .catch((failure: unknown) => current && setTargetsError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  const names = useMemo(() => new Map([...parties, ...products].map((item) => [item.id, item.name])), [parties, products]);
  const sellers = parties.filter((party) => party.is_seller);

  return (
    <>
      <PageHeader
        title="Comissões"
        description="O que cada vendedor ganhou nas vendas concluídas, pela regra mais específica que valia no dia da venda."
      />
      <Notice message={targetsError} />
      <StatementCard companyId={companyId} sellers={sellers} names={names} />
      <RulesSection
        companyId={companyId}
        canWrite={canWrite}
        parties={parties}
        products={products}
        names={names}
        targetsLoaded={targetsLoaded}
      />
    </>
  );
}

function StatementCard({ companyId, sellers, names }: { companyId: string; sellers: Party[]; names: Map<string, string> }) {
  const today = todayIso();
  const [from, setFrom] = useState(`${today.slice(0, 8)}01`);
  const [to, setTo] = useState(today);
  const [seller, setSeller] = useState("");
  const [statement, setStatement] = useState<CommissionStatement | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  // The server orders sellers by id, which means nothing to a person.
  const sortedSellers = [...(statement?.sellers ?? [])].sort((a, b) =>
    (names.get(a.seller_id) ?? "").localeCompare(names.get(b.seller_id) ?? "", "pt-BR"),
  );

  const valid = isIsoDate(from) && isIsoDate(to) && from <= to;

  useEffect(() => {
    if (!valid) {
      setLoading(false);
      return;
    }
    let current = true;
    // What was on screen belongs to other dates or another seller: never show it under the new ones.
    setStatement(null);
    setLoading(true);
    setError(null);
    api
      .commissionStatement(companyId, from, to, seller)
      .then((loaded) => current && setStatement(loaded))
      .catch((failure: unknown) => current && setError(errorMessage(failure)))
      .finally(() => current && setLoading(false));
    return () => {
      current = false;
    };
  }, [companyId, from, to, seller, valid]);

  return (
    <section className="card list-card" aria-label="Extrato de comissões">
      <h2>Extrato</h2>
      <div className="filters" role="search">
        <label>
          De
          <input type="date" value={from} onChange={(event) => setFrom(event.target.value)} />
        </label>
        <label>
          Até
          <input type="date" value={to} onChange={(event) => setTo(event.target.value)} />
        </label>
        <label>
          Vendedor
          <select value={seller} onChange={(event) => setSeller(event.target.value)}>
            <option value="">Todos</option>
            {sellers.map((party) => (
              <option key={party.id} value={party.id}>
                {party.name}
              </option>
            ))}
          </select>
        </label>
      </div>
      {!valid && <Notice message="Informe um período em que o primeiro dia não venha depois do último." />}
      <Notice message={error} />
      {loading && statement === null ? (
        <p role="status" className="status">
          <Spinner /> Carregando…
        </p>
      ) : statement !== null && valid && statement.sellers.length === 0 ? (
        <div className="empty">
          <HandCoinsIcon size={40} aria-hidden />
          <p role="status">Nenhuma comissão neste período.</p>
          <p className="muted">Só contam as vendas concluídas que têm vendedor.</p>
        </div>
      ) : (
        statement !== null &&
        valid && (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Vendedor</th>
                  <th className="num">Base</th>
                  <th className="num">Comissão</th>
                </tr>
              </thead>
              <tbody>
                {sortedSellers.map((entry) => (
                  <Fragment key={entry.seller_id}>
                    <tr>
                      <td className="cell-title">{names.get(entry.seller_id) ?? "Vendedor"}</td>
                      <td className="num">{formatMoney(entry.base)}</td>
                      <td className="num">{formatMoney(entry.commission)}</td>
                    </tr>
                    {entry.sales.map((sale) => (
                      <tr key={sale.sale_id} className="muted">
                        <td>
                          Venda {sale.number} · {formatDate(sale.sold_on)} · {names.get(sale.customer_id) ?? ""}
                        </td>
                        <td className="num">{formatMoney(sale.base)}</td>
                        <td className="num">{formatMoney(sale.commission)}</td>
                      </tr>
                    ))}
                  </Fragment>
                ))}
              </tbody>
              <tfoot>
                <tr>
                  <th scope="row">Total</th>
                  <td className="num">{formatMoney(statement.base)}</td>
                  <td className="num">{formatMoney(statement.commission)}</td>
                </tr>
              </tfoot>
            </table>
          </div>
        )
      )}
      <p className="muted">A base é o valor líquido dos itens, menos o que voltou em devoluções.</p>
    </section>
  );
}

interface RulesSectionProps {
  companyId: string;
  canWrite: boolean;
  parties: Party[];
  products: Product[];
  names: Map<string, string>;
  targetsLoaded: boolean;
}

function RulesSection({ companyId, canWrite, parties, products, names, targetsLoaded }: RulesSectionProps) {
  const [filters, setFilters] = useState<CommissionRuleFilters>(NO_COMMISSION_RULE_FILTERS);
  const [editing, setEditing] = useState<CommissionRule | null>(null);
  const [formVersion, setFormVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOf, setHistoryOf] = useState<string | null>(null);

  const { items, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<CommissionRule, CommissionRuleFilters>((applied, after) => api.listCommissionRules(companyId, applied, after), filters);
  const isFiltered = applied.seller !== "" || applied.product !== "" || applied.active !== "";
  const sellers = parties.filter((party) => party.is_seller);
  const productChoices = products.filter((product) => product.active || product.id === editing?.product_id);
  // A rule keeps naming a seller who stopped being one; dropping it from the choices would widen the rule to everybody.
  const sellerChoices = parties.filter((party) => party.is_seller || party.id === editing?.seller_id);

  function startEditing(rule: CommissionRule | null) {
    setEditing(rule);
    setFormError(null);
    setSaved(null);
    setFormVersion((version) => version + 1);
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (isAmbiguousDecimal(field("rate")) || toApiDecimal(field("rate")) === undefined) {
      setFormError("Informe o percentual, por exemplo 5 ou 7,5.");
      return;
    }
    if (!isIsoDate(field("starts_on"))) {
      setFormError("Informe o primeiro dia de venda que a regra cobre.");
      return;
    }
    if (field("ends_on") !== "" && (!isIsoDate(field("ends_on")) || field("ends_on") < field("starts_on"))) {
      setFormError("O último dia precisa ser uma data que não venha antes do primeiro.");
      return;
    }
    const input: CommissionRuleInput = {
      seller_id: field("seller_id") || undefined,
      product_id: field("product_id") || undefined,
      rate: toApiDecimal(field("rate")) ?? "",
      starts_on: field("starts_on"),
      ends_on: field("ends_on") || undefined,
      active: form.get("active") === "on",
    };
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      if (editing === null) {
        const created = await api.createCommissionRule(companyId, input);
        setHighlightedId(created.id);
        startEditing(null);
        setSaved(`Regra de ${rate(created.rate)} cadastrada.`);
        await reload();
      } else {
        const updated = await api.updateCommissionRule(companyId, editing.id, input);
        setHighlightedId(updated.id);
        startEditing(null);
        setSaved(`Regra de ${rate(updated.rate)} atualizada. As vendas já feitas são recalculadas no extrato.`);
        // The new dates or situation may move it in the list or out of a filter.
        await reload();
      }
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  const historyLoader = useCallback((id: string) => () => api.commissionRuleHistory(companyId, id), [companyId]);

  return (
    <div className={canWrite ? "split" : "split split-single"}>
      <section className="card list-card" aria-label="Regras de comissão">
        <h2>Regras</h2>
        <div className="filters" role="search">
          <label>
            Vendedor
            <select value={filters.seller} onChange={(event) => setFilters({ ...filters, seller: event.target.value })}>
              <option value="">Todos</option>
              {sellers.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Situação
            <select
              value={filters.active}
              onChange={(event) => setFilters({ ...filters, active: event.target.value as CommissionRuleFilters["active"] })}
            >
              <option value="">Todas</option>
              <option value="true">Ativas</option>
              <option value="false">Inativas</option>
            </select>
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
        ) : items.length === 0 && isFiltered ? (
          <div className="empty">
            <HandCoinsIcon size={40} aria-hidden />
            <p role="status">Nenhuma regra encontrada.</p>
            <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_COMMISSION_RULE_FILTERS })}>
              Limpar filtros
            </Button>
          </div>
        ) : items.length === 0 ? (
          <div className="empty">
            <HandCoinsIcon size={40} aria-hidden />
            <p>Nenhuma regra ainda.</p>
            <p className="muted">Sem regra, ninguém ganha comissão. Comece por uma regra geral e abra exceções.</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Vale para</th>
                  <th className="num">Percentual</th>
                  <th>Período</th>
                  <th>Situação</th>
                  <th>
                    <span className="visually-hidden">Ações</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {items.map((item, index) => (
                  <Fragment key={item.id}>
                    <ListRow index={index} highlighted={item.id === highlightedId}>
                      <td>
                        <div className="cell-title">{item.seller_id === null ? "Todos os vendedores" : (names.get(item.seller_id) ?? "Vendedor")}</div>
                        <div className="muted">{item.product_id === null ? "Todos os produtos" : (names.get(item.product_id) ?? "Produto")}</div>
                      </td>
                      <td className="num">{rate(item.rate)}</td>
                      <td>
                        desde {formatDate(item.starts_on)}
                        {item.ends_on !== null && <div className="muted">até {formatDate(item.ends_on)}</div>}
                      </td>
                      <td>
                        <StatusBadge active={item.active} feminine />
                      </td>
                      <td className="row-actions">
                        {canWrite && (
                          <Button
                            iconOnly
                            icon={SquarePenIcon}
                            type="button"
                            variant="ghost"
                            aria-label={`Alterar a regra de ${rate(item.rate)} desde ${formatDate(item.starts_on)}`}
                            disabled={saving}
                            onClick={() => startEditing(item)}
                          >
                            Alterar
                          </Button>
                        )}
                        <Button
                          iconOnly
                          type="button"
                          variant="ghost"
                          aria-label={`Histórico da regra de ${rate(item.rate)} desde ${formatDate(item.starts_on)}`}
                          aria-expanded={historyOf === item.id}
                          icon={HistoryIcon}
                          onClick={() => setHistoryOf(historyOf === item.id ? null : item.id)}
                        >
                          Histórico
                        </Button>
                      </td>
                    </ListRow>
                    {historyOf === item.id && (
                      <tr>
                        <td colSpan={5} role="region" aria-label={`Histórico da regra de ${rate(item.rate)}`}>
                          <History
                            load={historyLoader(item.id)}
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
          <h2>{editing === null ? "Nova regra" : "Alterar regra"}</h2>
          {targetsLoaded && (
            <form key={`${editing?.id ?? "new"}-${formVersion}`} onSubmit={(event) => void handleSubmit(event)}>
              <label>
                Vendedor
                <select name="seller_id" defaultValue={editing?.seller_id ?? ""}>
                  <option value="">Todos os vendedores</option>
                  {sellerChoices.map((party) => (
                    <option key={party.id} value={party.id}>
                      {party.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Produto
                <select name="product_id" defaultValue={editing?.product_id ?? ""}>
                  <option value="">Todos os produtos</option>
                  {productChoices.map((product) => (
                    <option key={product.id} value={product.id}>
                      {product.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Percentual (%)
                <input name="rate" inputMode="decimal" autoComplete="off" required placeholder="5" defaultValue={toInputDecimal(editing?.rate ?? null)} />
              </label>
              <label>
                Vale a partir de
                <input name="starts_on" type="date" required defaultValue={editing?.starts_on ?? todayIso()} />
              </label>
              <label>
                Vale até (opcional)
                <input name="ends_on" type="date" defaultValue={editing?.ends_on ?? ""} />
              </label>
              <label className="inline">
                <input name="active" type="checkbox" defaultChecked={editing?.active ?? true} /> Ativa
              </label>
              <p className="muted">
                Vale a regra mais específica: vendedor e produto, depois só o vendedor, depois só o produto, depois a geral.
                O dia que conta é o da venda.
              </p>
              <Notice message={formError} />
              <div className="form-actions">
                <Button type="submit" icon={editing === null ? PlusIcon : CheckIcon} pending={saving}>
                  {editing === null ? "Cadastrar regra" : "Salvar alterações"}
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
  );
}

/** A value of the trail as a person reads it: names for references, words for flags. */
function displayValue(field: string, value: string | null, names: Map<string, string>): string {
  if (value === null) return field === "seller_id" ? "Todos os vendedores" : field === "product_id" ? "Todos os produtos" : "—";
  if (field === "seller_id" || field === "product_id") return names.get(value) ?? value;
  if (field === "rate") return rate(value);
  if (field === "starts_on" || field === "ends_on") return formatDate(value);
  if (field === "active") return value === "true" ? "Ativa" : "Inativa";
  return value;
}
