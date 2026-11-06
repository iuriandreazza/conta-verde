import { Fragment, useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import {
  NO_RECURRENCE_FILTERS,
  api,
  errorMessage,
  type Category,
  type CostCenter,
  type EntryKind,
  type Recurrence,
  type RecurrenceFilters,
  type RecurrenceFrequency,
  type RecurrenceInput,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { RotateCCWIcon } from "../components/icons/rotate-ccw";
import { SquarePenIcon } from "../components/icons/square-pen";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { StatusBadge } from "../components/StatusBadge";
import { treeRows } from "../lib/categoryTree";
import { formatMoney, isAmbiguousDecimal, toInputDecimal } from "../lib/decimal";
import { centsToApi, formatDate, isIsoDate, toCents } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

const FREQUENCIES: { key: RecurrenceFrequency; label: string }[] = [
  { key: "weekly", label: "Toda semana" },
  { key: "monthly", label: "Todo mês" },
  { key: "yearly", label: "Todo ano" },
];

const FIELD_LABELS: Record<string, string> = {
  kind: "Tipo",
  description: "Descrição",
  party_id: "Cliente ou fornecedor",
  category_id: "Categoria",
  cost_center_id: "Centro de custo",
  amount: "Valor",
  frequency: "Frequência",
  starts_on: "Primeiro vencimento",
  ends_on: "Último dia",
  active: "Situação",
};

const frequencyLabel = (frequency: RecurrenceFrequency) =>
  FREQUENCIES.find((option) => option.key === frequency)?.label ?? frequency;

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function RecurrencesPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [filters, setFilters] = useState<RecurrenceFilters>(NO_RECURRENCE_FILTERS);
  const [editing, setEditing] = useState<Recurrence | null>(null);
  // Bumped after a save so the form starts over, empty or for another recurrence.
  const [formVersion, setFormVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);
  const [categories, setCategories] = useState<Category[]>([]);
  const [costCenters, setCostCenters] = useState<CostCenter[]>([]);
  const [parties, setParties] = useState<{ id: string; name: string }[]>([]);
  const [targetsError, setTargetsError] = useState<string | null>(null);
  const [targetsLoaded, setTargetsLoaded] = useState(false);

  // What a recurrence points at is read once: the form needs it, and so do the names in the list.
  useEffect(() => {
    let current = true;
    Promise.all([
      api.listCategories(companyId),
      readUpTo((after) => api.listCostCenters(companyId, { q: "", active: "", sort: "name" }, after)),
      readUpTo((after) => api.listParties(companyId, undefined, after)),
    ])
      .then(([categoryList, costCenterList, partyList]) => {
        if (current) {
          setCategories(categoryList.items);
          setCostCenters(costCenterList);
          setParties(partyList.map(({ id, name }) => ({ id, name })));
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
    for (const category of categories) byId.set(category.id, category.name);
    for (const center of costCenters) byId.set(center.id, center.name);
    for (const party of parties) byId.set(party.id, party.name);
    return byId;
  }, [categories, costCenters, parties]);

  const { items, setItems, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<Recurrence, RecurrenceFilters>(
      (applied, after) => api.listRecurrences(companyId, applied, after),
      filters,
    );
  const isFiltered = applied.q !== "" || applied.kind !== "" || applied.active !== "";

  // What is in use, plus what the recurrence already points at, so that changing one whose category was
  // deactivated since still shows it.
  const categoryRows = treeRows(categories).filter(
    ({ category }) =>
      category.id === editing?.category_id || (category.active && category.dre_group !== "transfers"),
  );
  const costCenterChoices = costCenters.filter(
    (center) => center.id === editing?.cost_center_id || center.active,
  );
  const partyChoices =
    editing?.party_id && !parties.some((party) => party.id === editing.party_id)
      ? [...parties, { id: editing.party_id, name: names.get(editing.party_id) ?? editing.party_id }]
      : parties;

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    const typedAmount = field("amount");
    // `1.000` could be a thousand or one: the person says which, as in every amount field.
    const cents = isAmbiguousDecimal(typedAmount) ? null : toCents(typedAmount);
    if (cents === null || cents === 0n) {
      setFormError("Informe o valor em reais e centavos, por exemplo 1.234,50.");
      return;
    }
    if (!isIsoDate(field("starts_on"))) {
      setFormError("Informe o primeiro vencimento.");
      return;
    }
    if (field("ends_on") !== "" && !isIsoDate(field("ends_on"))) {
      setFormError("O último dia não é uma data.");
      return;
    }
    if (field("ends_on") !== "" && field("ends_on") < field("starts_on")) {
      setFormError("O último dia não pode ser antes do primeiro vencimento.");
      return;
    }
    const input: RecurrenceInput = {
      kind: field("kind") as EntryKind,
      description: field("description"),
      party_id: field("party_id") || undefined,
      category_id: field("category_id"),
      cost_center_id: field("cost_center_id") || undefined,
      amount: centsToApi(cents),
      frequency: field("frequency") as RecurrenceFrequency,
      starts_on: field("starts_on"),
      ends_on: field("ends_on") || undefined,
      active: form.get("active") === "on",
    };
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      if (editing === null) {
        const created = await api.createRecurrence(companyId, input);
        setHighlightedId(created.id);
        setSaved(
          created.active
            ? `${created.description} cadastrada: os lançamentos que vencem nos próximos dois meses já foram gerados.`
            : `${created.description} cadastrada, pausada: nada será gerado até ativá-la.`,
        );
        setFormVersion((version) => version + 1);
        await reload();
      } else {
        const updated = await api.updateRecurrence(companyId, editing.id, input);
        // In place: the list keeps its page and its filters.
        setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
        setEditing(null);
        setSaved(`${updated.description} atualizada. O que já foi gerado não muda.`);
        setFormVersion((version) => version + 1);
      }
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  const historyLoader = useCallback(
    (id: string) => () => api.recurrenceHistory(companyId, id),
    [companyId],
  );

  return (
    <>
      <PageHeader
        title="Recorrências"
        description="Aluguel, assinaturas, mensalidades: o que se repete. Os lançamentos dos próximos dois meses são gerados sozinhos."
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
                placeholder="Descrição"
                autoComplete="off"
                maxLength={100}
                onChange={(event) => setFilters({ ...filters, q: event.target.value })}
              />
            </label>
            <label>
              Mostrar
              <select
                value={filters.kind}
                onChange={(event) => setFilters({ ...filters, kind: event.target.value as RecurrenceFilters["kind"] })}
              >
                <option value="">A receber e a pagar</option>
                <option value="receivable">Só a receber</option>
                <option value="payable">Só a pagar</option>
              </select>
            </label>
            <label>
              Situação
              <select
                value={filters.active}
                onChange={(event) => setFilters({ ...filters, active: event.target.value as RecurrenceFilters["active"] })}
              >
                <option value="">Ativas e pausadas</option>
                <option value="true">Só ativas</option>
                <option value="false">Só pausadas</option>
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
              <p role="status">Nenhuma recorrência encontrada.</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_RECURRENCE_FILTERS })}>
                Limpar filtros
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <RotateCCWIcon size={40} aria-hidden />
              <p>Nenhuma recorrência ainda.</p>
              <p className="muted">O aluguel, a assinatura do sistema, a mensalidade de um cliente.</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Descrição</th>
                    <th>Quando</th>
                    <th className="num">Valor</th>
                    <th>Situação</th>
                    <th>
                      <span className="visually-hidden">Ações</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((recurrence, index) => (
                    <Fragment key={recurrence.id}>
                      <ListRow index={index} highlighted={recurrence.id === highlightedId}>
                        <td>
                          <div className="cell-title">{recurrence.description}</div>
                          <div className="muted">
                            {recurrence.kind === "receivable" ? "A receber" : "A pagar"} ·{" "}
                            {names.get(recurrence.category_id) ?? ""}
                          </div>
                        </td>
                        <td>
                          {frequencyLabel(recurrence.frequency)}
                          <div className="muted">
                            desde {formatDate(recurrence.starts_on)}
                            {recurrence.ends_on !== null && ` até ${formatDate(recurrence.ends_on)}`}
                          </div>
                        </td>
                        <td className="num">{formatMoney(recurrence.amount)}</td>
                        <td>
                          <StatusBadge active={recurrence.active} feminine activeLabel="Ativa" inactiveLabel="Pausada" />
                        </td>
                        <td className="row-actions">
                          {canWrite && (
                            <Button
                              iconOnly
                              icon={SquarePenIcon}
                              type="button"
                              variant="ghost"
                              aria-label={`Alterar ${recurrence.description}`}
                              disabled={saving}
                              onClick={() => {
                                setEditing(recurrence);
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
                            aria-label={`Histórico de ${recurrence.description}`}
                            aria-expanded={historyOpenFor === recurrence.id}
                            aria-controls={`history-${recurrence.id}`}
                            icon={HistoryIcon}
                            onClick={() => setHistoryOpenFor(historyOpenFor === recurrence.id ? null : recurrence.id)}
                          >
                            Histórico
                          </Button>
                        </td>
                      </ListRow>
                      {historyOpenFor === recurrence.id && (
                        <tr id={`history-${recurrence.id}`}>
                          <td colSpan={5} role="region" aria-label={`Histórico de ${recurrence.description}`}>
                            <History
                              load={historyLoader(recurrence.id)}
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
            <h2>{editing === null ? "Nova recorrência" : `Alterar ${editing.description}`}</h2>
            {targetsLoaded && (
              <form
                // A new key starts the form over for another recurrence, or an empty one after a save.
                key={`${editing?.id ?? "new"}-${formVersion}`}
                onSubmit={(event) => void handleSubmit(event)}
              >
                <label>
                  Tipo
                  <select name="kind" defaultValue={editing?.kind ?? "payable"}>
                    <option value="payable">A pagar</option>
                    <option value="receivable">A receber</option>
                  </select>
                </label>
                <label>
                  Descrição
                  <input
                    name="description"
                    required
                    maxLength={200}
                    autoComplete="off"
                    autoFocus={editing !== null || formVersion > 0}
                    defaultValue={editing?.description}
                  />
                </label>
                <label>
                  Categoria
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
                <label>
                  Cliente ou fornecedor
                  <select name="party_id" defaultValue={editing?.party_id ?? ""}>
                    <option value="">Nenhum</option>
                    {partyChoices.map((party) => (
                      <option key={party.id} value={party.id}>
                        {party.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Valor de cada vez
                  <input
                    name="amount"
                    inputMode="decimal"
                    autoComplete="off"
                    required
                    placeholder="0,00"
                    defaultValue={editing ? toInputDecimal(editing.amount) : ""}
                  />
                </label>
                <label>
                  Frequência
                  <select name="frequency" defaultValue={editing?.frequency ?? "monthly"}>
                    {FREQUENCIES.map((option) => (
                      <option key={option.key} value={option.key}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Primeiro vencimento
                  <input name="starts_on" type="date" required defaultValue={editing?.starts_on ?? ""} />
                </label>
                <label>
                  Último dia (opcional)
                  <input name="ends_on" type="date" defaultValue={editing?.ends_on ?? ""} />
                </label>
                <label className="inline">
                  <input name="active" type="checkbox" defaultChecked={editing?.active ?? true} /> Ativa
                </label>
                <p className="muted">
                  Mudar a recorrência vale para os próximos lançamentos; os que já foram gerados não mudam.
                </p>
                <Notice message={formError} />
                <div className="form-actions">
                  <Button type="submit" icon={editing === null ? PlusIcon : CheckIcon} pending={saving}>
                    {editing === null ? "Adicionar" : "Salvar alterações"}
                  </Button>
                  {editing !== null && (
                    <Button
                      type="button"
                      variant="ghost"
                      disabled={saving}
                      onClick={() => {
                        setEditing(null);
                        setFormError(null);
                      }}
                    >
                      Cancelar
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

/** A value of the trail as a person reads it: names for references, words for keys, `R$` for the amount. */
function displayValue(field: string, value: string | null, names: Map<string, string>): string {
  if (value === null) return "—";
  if (["party_id", "category_id", "cost_center_id"].includes(field)) return names.get(value) ?? value;
  if (field === "kind") return value === "receivable" ? "A receber" : "A pagar";
  if (field === "frequency") return frequencyLabel(value as RecurrenceFrequency);
  if (field === "amount") return formatMoney(value);
  if (field === "starts_on" || field === "ends_on") return formatDate(value);
  if (field === "active") return value === "true" ? "Ativa" : "Pausada";
  return value;
}
