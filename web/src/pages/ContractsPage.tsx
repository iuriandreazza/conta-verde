import { Fragment, useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import {
  NO_CONTRACT_FILTERS,
  NO_FILTERS,
  NO_PAYMENT_TERM_FILTERS,
  NO_PRODUCT_FILTERS,
  api,
  errorMessage,
  type BillingCycle,
  type Category,
  type Contract,
  type ContractFilters,
  type ContractInput,
  type ContractStatus,
  type CostCenter,
  type Party,
  type PaymentTerm,
  type Product,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { RotateCCWIcon } from "../components/icons/rotate-ccw";
import { SquarePenIcon } from "../components/icons/square-pen";
import { LinesEditor, linesProblem, linesToInput, newLine, rowsFromLines, type LineRow } from "../components/LinesEditor";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { treeRows } from "../lib/categoryTree";
import { CONTRACT_STATUS_LABELS, CYCLE_LABELS } from "../lib/contractCycle";
import { formatMoney, isAmbiguousDecimal, toApiDecimal } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { todayIso } from "../lib/settlement";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

const FIELD_LABELS: Record<string, string> = {
  name: "Nome",
  customer_id: "Cliente",
  seller_id: "Vendedor",
  payment_term_id: "Prazo de pagamento",
  category_id: "Categoria",
  cost_center_id: "Centro de custo",
  notes: "Observações",
  lines: "Itens",
  cycle: "Ciclo",
  starts_on: "Primeira cobrança",
  ends_on: "Última cobrança",
  status: "Situação",
};

type Panel = { kind: "adjust" | "terminate" | "history"; id: string };

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function ContractsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("sales");
  const [filters, setFilters] = useState<ContractFilters>(NO_CONTRACT_FILTERS);
  const [editing, setEditing] = useState<Contract | null>(null);
  const [formVersion, setFormVersion] = useState(0);
  const [rows, setRows] = useState<LineRow[]>(() => [newLine()]);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [panel, setPanel] = useState<Panel | null>(null);
  const [acting, setActing] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [parties, setParties] = useState<Party[]>([]);
  const [products, setProducts] = useState<Product[]>([]);
  const [terms, setTerms] = useState<PaymentTerm[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  const [costCenters, setCostCenters] = useState<CostCenter[]>([]);
  const [targetsError, setTargetsError] = useState<string | null>(null);
  const [targetsLoaded, setTargetsLoaded] = useState(false);

  useEffect(() => {
    let current = true;
    Promise.all([
      readUpTo((after) => api.listParties(companyId, NO_FILTERS, after)),
      readUpTo((after) => api.listProducts(companyId, NO_PRODUCT_FILTERS, after)),
      readUpTo((after) => api.listPaymentTerms(companyId, NO_PAYMENT_TERM_FILTERS, after)),
      api.listCategories(companyId),
      readUpTo((after) => api.listCostCenters(companyId, { q: "", active: "", sort: "name" }, after)),
    ])
      .then(([partyList, productList, termList, categoryList, centerList]) => {
        if (current) {
          setParties(partyList);
          setProducts(productList);
          setTerms(termList);
          setCategories(categoryList.items);
          setCostCenters(centerList);
          setTargetsLoaded(true);
        }
      })
      .catch((failure: unknown) => current && setTargetsError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  const names = useMemo(() => {
    const byId = new Map<string, string>();
    for (const item of [...parties, ...terms, ...products, ...categories, ...costCenters]) {
      byId.set(item.id, item.name);
    }
    return byId;
  }, [parties, terms, products, categories, costCenters]);

  const { items, setItems, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<Contract, ContractFilters>((applied, after) => api.listContracts(companyId, applied, after), filters);
  const isFiltered = applied.q !== "" || applied.status !== "";

  const inContract = new Set(rows.map((row) => row.product_id));
  const productChoices = products.filter((product) => product.active || inContract.has(product.id));
  const termChoices = terms.filter((term) => term.active || term.id === editing?.payment_term_id);
  const categoryRows = treeRows(categories).filter(
    ({ category }) => category.id === editing?.category_id || (category.active && category.dre_group !== "transfers"),
  );
  const costCenterChoices = costCenters.filter((center) => center.id === editing?.cost_center_id || center.active);
  const sellers = parties.filter((party) => party.is_seller);

  function startEditing(contract: Contract | null) {
    setEditing(contract);
    setRows(contract === null ? [newLine()] : rowsFromLines(contract.lines));
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
    if (!isIsoDate(field("starts_on"))) {
      setFormError("Informe a data da primeira cobrança.");
      return;
    }
    if (field("ends_on") !== "" && (!isIsoDate(field("ends_on")) || field("ends_on") < field("starts_on"))) {
      setFormError("A última cobrança precisa ser uma data que não venha antes da primeira.");
      return;
    }
    const problem = linesProblem(rows);
    if (problem !== null) {
      setFormError(problem);
      return;
    }
    const input: ContractInput = {
      name: field("name"),
      customer_id: field("customer_id"),
      seller_id: field("seller_id") || undefined,
      payment_term_id: field("payment_term_id") || undefined,
      category_id: field("category_id"),
      cost_center_id: field("cost_center_id") || undefined,
      notes: field("notes") || undefined,
      lines: linesToInput(rows),
      cycle: field("cycle") as BillingCycle,
      starts_on: field("starts_on"),
      ends_on: field("ends_on") || undefined,
      active: form.get("active") === "on",
    };
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      if (editing === null) {
        const created = await api.createContract(companyId, input);
        setHighlightedId(created.id);
        startEditing(null);
        setSaved(`${created.name} cadastrado: os ciclos que já venceram foram cobrados.`);
        await reload();
      } else {
        const updated = await api.updateContract(companyId, editing.id, input);
        setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
        startEditing(null);
        setSaved(`${updated.name} atualizado. O que já foi cobrado não muda.`);
      }
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  async function act(run: () => Promise<Contract>, message: (done: Contract) => string) {
    if (acting) {
      return;
    }
    setActing(true);
    setActionError(null);
    setSaved(null);
    try {
      const updated = await run();
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      // A form open for this contract would save its old prices or its old status over what was just done.
      if (editing?.id === updated.id) {
        startEditing(null);
      }
      setPanel(null);
      setSaved(message(updated));
    } catch (failure) {
      setActionError(errorMessage(failure));
    } finally {
      setActing(false);
    }
  }

  const historyLoader = useCallback((id: string) => () => api.contractHistory(companyId, id), [companyId]);
  const toggle = (kind: Panel["kind"], id: string) => {
    setPanel(panel?.kind === kind && panel.id === id ? null : { kind, id });
    setActionError(null);
  };

  return (
    <>
      <PageHeader
        title="Contratos"
        description="Cobranças que se repetem: cada ciclo vira uma venda concluída e a conta a receber, sozinhos, no dia."
      />
      <div className={canWrite ? "split" : "split split-single"}>
        <section className="card list-card">
          <h2>Cadastrados</h2>
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
                value={filters.status}
                onChange={(event) => setFilters({ ...filters, status: event.target.value as ContractFilters["status"] })}
              >
                <option value="">Todas</option>
                {Object.entries(CONTRACT_STATUS_LABELS).map(([key, label]) => (
                  <option key={key} value={key}>
                    {label}
                  </option>
                ))}
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
              <RotateCCWIcon size={40} aria-hidden />
              <p role="status">Nenhum contrato encontrado.</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_CONTRACT_FILTERS })}>
                Limpar filtros
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <RotateCCWIcon size={40} aria-hidden />
              <p>Nenhum contrato ainda.</p>
              <p className="muted">Uma mensalidade, uma manutenção, uma assinatura: descreva uma vez e ele se cobra.</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Contrato</th>
                    <th>Ciclo</th>
                    <th className="num">Cada ciclo</th>
                    <th>Situação</th>
                    <th>
                      <span className="visually-hidden">Ações</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((contract, index) => (
                    <Fragment key={contract.id}>
                      <ListRow index={index} highlighted={contract.id === highlightedId}>
                        <td>
                          <div className="cell-title">{contract.name}</div>
                          <div className="muted">{names.get(contract.customer_id) ?? ""}</div>
                        </td>
                        <td>
                          {CYCLE_LABELS[contract.cycle]}
                          <div className="muted">
                            desde {formatDate(contract.starts_on)}
                            {contract.ends_on !== null && ` até ${formatDate(contract.ends_on)}`}
                          </div>
                        </td>
                        <td className="num">{formatMoney(contract.total)}</td>
                        <td>
                          <span className={statusClass(contract.status)}>{CONTRACT_STATUS_LABELS[contract.status]}</span>
                          {contract.billed_through !== null && (
                            <div className="muted">cobrado até {formatDate(contract.billed_through)}</div>
                          )}
                        </td>
                        <td className="row-actions">
                          {canWrite && contract.status !== "terminated" && (
                            <>
                              <Button
                                iconOnly
                                icon={SquarePenIcon}
                                type="button"
                                variant="ghost"
                                aria-label={`Alterar ${contract.name}`}
                                disabled={saving}
                                onClick={() => startEditing(contract)}
                              >
                                Alterar
                              </Button>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={`Reajustar ${contract.name}`}
                                aria-expanded={panel?.kind === "adjust" && panel.id === contract.id}
                                onClick={() => toggle("adjust", contract.id)}
                              >
                                Reajustar
                              </Button>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={`Encerrar ${contract.name}`}
                                aria-expanded={panel?.kind === "terminate" && panel.id === contract.id}
                                onClick={() => toggle("terminate", contract.id)}
                              >
                                Encerrar
                              </Button>
                            </>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={`Histórico de ${contract.name}`}
                            aria-expanded={panel?.kind === "history" && panel.id === contract.id}
                            icon={HistoryIcon}
                            onClick={() => toggle("history", contract.id)}
                          >
                            Histórico
                          </Button>
                        </td>
                      </ListRow>
                      {panel?.id === contract.id && panel.kind === "adjust" && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={`Reajuste de ${contract.name}`}>
                            <form
                              className="opening-form"
                              onSubmit={(event) => {
                                event.preventDefault();
                                const typed = String(new FormData(event.currentTarget).get("percent") ?? "");
                                if (isAmbiguousDecimal(typed) || toApiDecimal(typed) === undefined) {
                                  setActionError("Informe a porcentagem, por exemplo 5,5 ou -3.");
                                  return;
                                }
                                void act(
                                  () => api.adjustContract(companyId, contract.id, toApiDecimal(typed) ?? ""),
                                  (done) => `${done.name} reajustado: cada ciclo agora cobra ${formatMoney(done.total)}, a partir do próximo.`,
                                );
                              }}
                            >
                              <label>
                                Reajuste (%)
                                <input name="percent" inputMode="decimal" autoComplete="off" placeholder="5,5" required />
                              </label>
                              <Button type="submit" icon={CheckIcon} pending={acting}>
                                Aplicar reajuste
                              </Button>
                              <p className="muted">
                                Muda o preço de cada item (arredondado ao centavo) a partir do próximo ciclo; o que já foi
                                cobrado não muda.
                              </p>
                              <Notice message={actionError} />
                            </form>
                          </td>
                        </tr>
                      )}
                      {panel?.id === contract.id && panel.kind === "terminate" && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={`Encerramento de ${contract.name}`}>
                            <form
                              className="opening-form"
                              onSubmit={(event) => {
                                event.preventDefault();
                                const on = String(new FormData(event.currentTarget).get("on") ?? "");
                                if (!isIsoDate(on)) {
                                  setActionError("Informe o último dia de cobrança.");
                                  return;
                                }
                                void act(
                                  () => api.terminateContract(companyId, contract.id, on),
                                  (done) => `${done.name} encerrado: nada será cobrado depois de ${formatDate(on)}.`,
                                );
                              }}
                            >
                              <label>
                                Último dia de cobrança
                                <input name="on" type="date" required defaultValue={todayIso()} />
                              </label>
                              <Button type="submit" pending={acting}>
                                Encerrar contrato
                              </Button>
                              <p className="muted">Encerrar não se desfaz: para voltar a cobrar, cadastre outro contrato.</p>
                              <Notice message={actionError} />
                            </form>
                          </td>
                        </tr>
                      )}
                      {panel?.id === contract.id && panel.kind === "history" && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={`Histórico de ${contract.name}`}>
                            <History
                              load={historyLoader(contract.id)}
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
            <h2>{editing === null ? "Novo contrato" : `Alterar ${editing.name}`}</h2>
            {targetsLoaded && (
              <form key={`${editing?.id ?? "new"}-${formVersion}`} onSubmit={(event) => void handleSubmit(event)}>
                <label>
                  Nome
                  <input name="name" required maxLength={200} autoComplete="off" defaultValue={editing?.name} />
                </label>
                <label>
                  Cliente
                  <select name="customer_id" required defaultValue={editing?.customer_id ?? ""}>
                    <option value="" disabled>
                      Escolha um cliente
                    </option>
                    {parties.map((party) => (
                      <option key={party.id} value={party.id}>
                        {party.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Vendedor
                  <select name="seller_id" defaultValue={editing?.seller_id ?? ""}>
                    <option value="">Nenhum</option>
                    {sellers.map((party) => (
                      <option key={party.id} value={party.id}>
                        {party.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Ciclo de cobrança
                  <select name="cycle" defaultValue={editing?.cycle ?? "monthly"}>
                    {Object.entries(CYCLE_LABELS).map(([key, label]) => (
                      <option key={key} value={key}>
                        {label}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Primeira cobrança
                  <input name="starts_on" type="date" required defaultValue={editing?.starts_on ?? todayIso()} />
                </label>
                <label>
                  Última cobrança (opcional)
                  <input name="ends_on" type="date" defaultValue={editing?.ends_on ?? ""} />
                </label>
                <label>
                  Prazo de pagamento
                  <select name="payment_term_id" defaultValue={editing?.payment_term_id ?? ""}>
                    <option value="">À vista, no dia da cobrança</option>
                    {termChoices.map((term) => (
                      <option key={term.id} value={term.id}>
                        {term.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Categoria da receita
                  <select name="category_id" required defaultValue={editing?.category_id ?? ""}>
                    <option value="" disabled>
                      Escolha uma categoria
                    </option>
                    {categoryRows.map(({ category, depth }) => (
                      <option key={category.id} value={category.id}>
                        {"— ".repeat(depth)}
                        {category.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Centro de custo
                  <select name="cost_center_id" defaultValue={editing?.cost_center_id ?? ""}>
                    <option value="">Nenhum</option>
                    {costCenterChoices.map((center) => (
                      <option key={center.id} value={center.id}>
                        {center.name}
                      </option>
                    ))}
                  </select>
                </label>
                <LinesEditor companyId={companyId} rows={rows} onChange={setRows} products={productChoices} />
                <label>
                  Observações
                  <textarea name="notes" rows={3} maxLength={2000} defaultValue={editing?.notes ?? ""} />
                </label>
                <label className="inline">
                  <input name="active" type="checkbox" defaultChecked={editing ? editing.status === "active" : true} /> Ativo
                </label>
                <p className="muted">Mudar o contrato vale para as próximas cobranças; o que já foi cobrado não muda.</p>
                <Notice message={formError} />
                <div className="form-actions">
                  <Button type="submit" icon={editing === null ? PlusIcon : CheckIcon} pending={saving}>
                    {editing === null ? "Cadastrar contrato" : "Salvar alterações"}
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

function statusClass(status: ContractStatus): string {
  switch (status) {
    case "active":
      return "badge badge-dot badge-ok";
    case "terminated":
      return "badge badge-dot badge-critical";
    default:
      return "badge badge-dot badge-muted";
  }
}

/** A value of the trail as a person reads it: names for references, words for keys. */
function displayValue(field: string, value: string | null, names: Map<string, string>): string {
  if (value === null) return "—";
  if (["customer_id", "seller_id", "payment_term_id", "category_id", "cost_center_id"].includes(field)) {
    return names.get(value) ?? value;
  }
  if (field === "status") return CONTRACT_STATUS_LABELS[value as ContractStatus] ?? value;
  if (field === "cycle") return CYCLE_LABELS[value as BillingCycle] ?? value;
  if (field === "starts_on" || field === "ends_on") return formatDate(value);
  if (field === "lines") {
    return value
      .split("\n")
      .map((line) => {
        // The description comes last and may hold colons of its own.
        const [product, quantity, price, discount, ...description] = line.split(":");
        const name = description.length > 0 ? description.join(":") : (names.get(product) ?? "item");
        return `${name} × ${quantity.replace(".", ",")} a ${formatMoney(price)}${discount === "0" ? "" : ` (− ${formatMoney(discount)})`}`;
      })
      .join("; ");
  }
  return value;
}
