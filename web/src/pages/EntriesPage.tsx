import { Fragment, useEffect, useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_ENTRY_FILTERS,
  api,
  errorMessage,
  type BankAccount,
  type Category,
  type CostCenter,
  type Entry,
  type EntryFilters,
  type EntryInput,
  type PaymentMethod,
} from "../api";
import { Button } from "../components/Button";
import { EntryForm, type EntryTargets } from "../components/EntryForm";
import { EntryHistory } from "../components/EntryHistory";
import { ListRow } from "../components/ListRow";
import { AttachmentsPanel } from "../components/AttachmentsPanel";
import { SettlementPanel } from "../components/SettlementPanel";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { CircleDollarSignIcon } from "../components/icons/circle-dollar-sign";
import { FileTextIcon } from "../components/icons/file-text";
import { SquarePenIcon } from "../components/icons/square-pen";
import { treeRows } from "../lib/categoryTree";
import { formatMoney } from "../lib/decimal";
import { STATUS_LABELS, entryStatus, todayIso, type InstallmentStatus } from "../lib/settlement";
import { formatDate } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { usePagedList } from "../lib/usePagedList";
import { useCanWrite } from "../lib/useCanWrite";

const STATUS_BADGES: Record<InstallmentStatus, string> = {
  open: "badge badge-dot badge-muted",
  partial: "badge badge-dot badge-warning",
  settled: "badge badge-dot badge-ok",
  overdue: "badge badge-dot badge-critical",
};

function describeDue(entry: Entry): string {
  const [first] = entry.installments;
  const last = entry.installments[entry.installments.length - 1];
  return entry.installments.length === 1
    ? formatDate(first.due_date)
    : `${entry.installments.length} parcelas, de ${formatDate(first.due_date)} a ${formatDate(last.due_date)}`;
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function EntriesPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [filters, setFilters] = useState<EntryFilters>(NO_ENTRY_FILTERS);
  const [editing, setEditing] = useState<Entry | null>(null);
  // Bumped after a save so the form starts over, empty or for another entry.
  const [formVersion, setFormVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);
  const [settlementsOpenFor, setSettlementsOpenFor] = useState<string | null>(null);
  const [attachmentsOpenFor, setAttachmentsOpenFor] = useState<string | null>(null);
  const [banks, setBanks] = useState<BankAccount[]>([]);
  const [methods, setMethods] = useState<PaymentMethod[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  const [costCenters, setCostCenters] = useState<CostCenter[]>([]);
  const [parties, setParties] = useState<{ id: string; name: string }[]>([]);
  const [targetsError, setTargetsError] = useState<string | null>(null);
  const [targetsLoaded, setTargetsLoaded] = useState(false);

  // What an entry points at is read once: the filter by category and the form both need it.
  useEffect(() => {
    let current = true;
    Promise.all([
      api.listCategories(companyId),
      readUpTo((after) => api.listCostCenters(companyId, { q: "", active: "", sort: "name" }, after)),
      readUpTo((after) => api.listParties(companyId, undefined, after)),
      readUpTo((after) => api.listBankAccounts(companyId, undefined, after)),
      readUpTo((after) => api.listPaymentMethods(companyId, undefined, after)),
    ])
      .then(([categoryList, costCenterList, partyList, bankList, methodList]) => {
        if (current) {
          setCategories(categoryList.items);
          setBanks(bankList);
          setMethods(methodList);
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

  const targets: EntryTargets = { categories, costCenters, parties };
  const names = useMemo(() => {
    const byId = new Map<string, string>();
    for (const category of categories) byId.set(category.id, category.name);
    for (const center of costCenters) byId.set(center.id, center.name);
    for (const party of parties) byId.set(party.id, party.name);
    return byId;
  }, [categories, costCenters, parties]);

  const {
    items: entries,
    setItems,
    nextCursor,
    loading,
    loadingMore,
    error,
    applied,
    reload,
    loadMore,
  } = usePagedList<Entry, EntryFilters>(
    (applied, after) => api.listEntries(companyId, applied, after),
    filters,
  );
  const today = todayIso();
  const isFiltered = applied.q !== "" || applied.kind !== "" || applied.category !== "";

  /**
   * Reads one entry again and puts it where it is in the list, so a settlement does not send the
   * list back to its first page and lose the "load more" pages.
   */
  async function refresh(id: string) {
    try {
      const fresh = await api.getEntry(companyId, id);
      setItems((current) => current.map((item) => (item.id === fresh.id ? fresh : item)));
    } catch {
      await reload();
    }
  }

  async function handleSubmit(input: EntryInput) {
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      if (editing === null) {
        const created = await api.createEntry(companyId, input);
        setHighlightedId(created.id);
        setSaved(`${created.description} adicionado.`);
        setFormVersion((version) => version + 1);
        await reload();
      } else {
        const updated = await api.updateEntry(companyId, editing.id, input);
        setEditing(null);
        setSaved(`${updated.description} atualizado.`);
        setFormVersion((version) => version + 1);
        // Read again, not changed in place: a new date, kind or category can move the entry to
        // another place in the list, or out of the filter the list is showing.
        await reload();
      }
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <PageHeader
        title="Lançamentos"
        description="O que a empresa tem a receber e a pagar, dividido em parcelas."
      />
      <div className={canWrite ? "split" : "split split-single"}>
        <section className="card list-card">
          <h2>Lançados</h2>
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
                onChange={(event) =>
                  setFilters({ ...filters, kind: event.target.value as EntryFilters["kind"] })
                }
              >
                <option value="">A receber e a pagar</option>
                <option value="receivable">Só a receber</option>
                <option value="payable">Só a pagar</option>
              </select>
            </label>
            <label>
              Categoria
              <select
                value={filters.category}
                onChange={(event) => setFilters({ ...filters, category: event.target.value })}
              >
                <option value="">Todas</option>
                {treeRows(categories).map(({ category, depth }) => (
                  <option key={category.id} value={category.id}>
                    {"— ".repeat(depth)}
                    {category.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Ordem
              <select
                value={filters.sort}
                onChange={(event) =>
                  setFilters({ ...filters, sort: event.target.value as EntryFilters["sort"] })
                }
              >
                <option value="-competence_date">Competência (mais recente)</option>
                <option value="competence_date">Competência (mais antiga)</option>
              </select>
            </label>
          </div>
          <Notice message={error ?? targetsError} />
          {loading ? (
            <p role="status" className="status">
              <Spinner /> Carregando…
            </p>
          ) : entries.length === 0 && error ? (
            <Button type="button" variant="ghost" onClick={() => void reload()}>
              Tentar de novo
            </Button>
          ) : entries.length === 0 && isFiltered ? (
            <div className="empty">
              <BriefcaseBusinessIcon size={40} aria-hidden />
              <p role="status">Nenhum lançamento encontrado.</p>
              <Button
                type="button"
                variant="ghost"
                onClick={() => setFilters({ ...NO_ENTRY_FILTERS, sort: filters.sort })}
              >
                Limpar filtros
              </Button>
            </div>
          ) : entries.length === 0 ? (
            <div className="empty">
              <BriefcaseBusinessIcon size={40} aria-hidden />
              <p>Nenhum lançamento ainda.</p>
              <p className="muted">
                Uma venda a prazo, o aluguel do mês, uma conta de luz: cada um é um lançamento com
                suas parcelas.
              </p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Descrição</th>
                    <th>Competência</th>
                    <th>Vencimento</th>
                    <th className="num">Total</th>
                    <th>
                      <span className="visually-hidden">Ações</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map((entry, index) => {
                    const status = entryStatus(entry, today);
                    return (
                    <Fragment key={entry.id}>
                      <ListRow index={index} highlighted={entry.id === highlightedId}>
                        <td>
                          <div className="cell-title">{entry.description}</div>
                          <div className="muted">
                            <span className={STATUS_BADGES[status]}>{STATUS_LABELS[status]}</span>{" "}
                            {entry.kind === "receivable" ? "A receber" : "A pagar"} ·{" "}
                            {names.get(entry.category_id) ?? ""}
                            {entry.recurrence_id !== null && " · Recorrente"}
                          </div>
                        </td>
                        <td className="tabular">{formatDate(entry.competence_date)}</td>
                        <td className="tabular">{describeDue(entry)}</td>
                        <td className="num">{formatMoney(entry.total)}</td>
                        <td className="row-actions">
                          {canWrite && (
                            <Button
                              iconOnly
                              icon={SquarePenIcon}
                              type="button"
                              variant="ghost"
                              aria-label={`Alterar ${entry.description}`}
                              disabled={saving}
                              onClick={() => {
                                setEditing(entry);
                                setFormError(null);
                                setSaved(null);
                              }}
                            >
                              Alterar
                            </Button>
                          )}
                          <Button
                            iconOnly
                            icon={CircleDollarSignIcon}
                            type="button"
                            variant="ghost"
                            aria-label={`Baixas de ${entry.description}`}
                            aria-expanded={settlementsOpenFor === entry.id}
                            aria-controls={`settlements-${entry.id}`}
                            onClick={() =>
                              setSettlementsOpenFor(settlementsOpenFor === entry.id ? null : entry.id)
                            }
                          >
                            Baixas
                          </Button>
                          <Button
                            iconOnly
                            icon={FileTextIcon}
                            type="button"
                            variant="ghost"
                            aria-label={`Anexos de ${entry.description}`}
                            aria-expanded={attachmentsOpenFor === entry.id}
                            aria-controls={`attachments-${entry.id}`}
                            onClick={() =>
                              setAttachmentsOpenFor(attachmentsOpenFor === entry.id ? null : entry.id)
                            }
                          >
                            Anexos
                          </Button>
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={`Histórico de ${entry.description}`}
                            aria-expanded={historyOpenFor === entry.id}
                            aria-controls={`history-${entry.id}`}
                            icon={HistoryIcon}
                            onClick={() =>
                              setHistoryOpenFor(historyOpenFor === entry.id ? null : entry.id)
                            }
                          >
                            Histórico
                          </Button>
                        </td>
                      </ListRow>
                      {settlementsOpenFor === entry.id && (
                        <tr id={`settlements-${entry.id}`}>
                          <td colSpan={5} role="region" aria-label={`Baixas de ${entry.description}`}>
                            <SettlementPanel
                              companyId={companyId}
                              entry={entry}
                              banks={banks}
                              methods={methods}
                              canWrite={canWrite}
                              onChanged={() => void refresh(entry.id)}
                            />
                          </td>
                        </tr>
                      )}
                      {attachmentsOpenFor === entry.id && (
                        <tr id={`attachments-${entry.id}`}>
                          <td colSpan={5} role="region" aria-label={`Anexos de ${entry.description}`}>
                            <AttachmentsPanel companyId={companyId} entryId={entry.id} canWrite={canWrite} />
                          </td>
                        </tr>
                      )}
                      {historyOpenFor === entry.id && (
                        <tr id={`history-${entry.id}`}>
                          <td colSpan={5} role="region" aria-label={`Histórico de ${entry.description}`}>
                            <EntryHistory
                              // Remounted when the entry changes, so an open history follows an edit.
                              key={JSON.stringify(entry)}
                              companyId={companyId}
                              entryId={entry.id}
                              nameOf={(id) => names.get(id)}
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
          <aside className="card form-card">
            <h2>{editing === null ? "Novo lançamento" : `Alterar ${editing.description}`}</h2>
            {targetsLoaded ? (
              <EntryForm
                // A new key starts the form over for another entry, or an empty one after a save.
                key={`${editing?.id ?? "new"}-${formVersion}`}
                entry={editing}
                targets={targets}
                // After a save or when editing, the person is in the form: put the cursor back in it.
                focusDescription={editing !== null || formVersion > 0}
                pending={saving}
                error={formError}
                onSubmit={(input) => void handleSubmit(input)}
                onCancel={() => {
                  setEditing(null);
                  setFormError(null);
                }}
              />
            ) : targetsError === null ? (
              // The choices are read before the form shows, so nothing typed is lost when they arrive.
              <p role="status" className="status">
                <Spinner /> Carregando…
              </p>
            ) : null}
            <Notice message={saved} tone="success" />
          </aside>
        )}
      </div>
    </>
  );
}
