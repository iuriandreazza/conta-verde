import { Fragment, useCallback, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_RECURRENCE_FILTERS,
  api,
  errorMessage,
  type Recurrence,
  type RecurrenceFilters,
  type RecurrenceFrequency,
  type RecurrenceInput,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { RotateCCWIcon } from "../components/icons/rotate-ccw";
import { SquarePenIcon } from "../components/icons/square-pen";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { RecurrenceForm } from "../components/RecurrenceForm";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { frequencyLabel } from "../lib/recurrence";
import { useEntryTargets } from "../lib/useEntryTargets";
import { useLastValue } from "../lib/useLastValue";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { StatusBadge } from "../components/StatusBadge";
import { formatMoney } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";


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


// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function RecurrencesPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [filters, setFilters] = useState<RecurrenceFilters>(NO_RECURRENCE_FILTERS);
  const [editing, setEditing] = useState<Recurrence | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);
  const { categories, costCenters, parties, names, error: targetsError, loaded: targetsLoaded } = useEntryTargets(companyId, { payments: false });

  const { items, setItems, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<Recurrence, RecurrenceFilters>(
      (applied, after) => api.listRecurrences(companyId, applied, after),
      filters,
    );
  const isFiltered = applied.q !== "" || applied.kind !== "" || applied.active !== "";

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A recurrence made from the top bar or from here: read the list again and flash the new row.
  useOnCreated("recurrence", (created) => {
    setSaved(null);
    setHighlightedId(created.id);
    void reload();
  });

  function closeSheet() {
    // A change on its way finishes first: closing now would hide what a failure has to say.
    if (saving) return;
    setEditing(null);
    setFormError(null);
  }

  async function handleSubmit(input: RecurrenceInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updateRecurrence(companyId, editing.id, input);
      // In place: the list keeps its page and its filters.
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setEditing(null);
      setSaved(`${updated.description} atualizada. O que já foi gerado não muda.`);
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
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("recurrence")}>
            Nova recorrência
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
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
          <Sheet open={editing !== null} title={`Alterar ${shown?.description ?? ""}`} onClose={closeSheet}>
            {shown !== null &&
              (targetsLoaded ? (
                <RecurrenceForm
                  key={shown.id}
                  recurrence={shown}
                  targets={{ categories, costCenters, parties, names }}
                  pending={saving}
                  error={formError}
                  onSubmit={(input) => void handleSubmit(input)}
                  onCancel={closeSheet}
                />
              ) : targetsError === null ? (
                <p role="status" className="status">
                  <Spinner /> Carregando…
                </p>
              ) : null)}
          </Sheet>
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
