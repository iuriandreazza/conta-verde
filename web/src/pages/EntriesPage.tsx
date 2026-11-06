import { Fragment, useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import {
  NO_ENTRY_FILTERS,
  api,
  errorMessage,
  type Entry,
  type EntryFilters,
  type EntryKind,
  type EntryInput,
} from "../api";
import { PlusIcon } from "../components/icons/plus";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { useLastValue } from "../lib/useLastValue";
import { useEntryTargets } from "../lib/useEntryTargets";
import { ListFooter } from "../components/ListFooter";
import { Button } from "../components/Button";
import { EntryForm } from "../components/EntryForm";
import { EntryHistory } from "../components/EntryHistory";
import { ListRow } from "../components/ListRow";
import { AttachmentsPanel } from "../components/AttachmentsPanel";
import { ChargesPanel } from "../components/ChargesPanel";
import { SettlementPanel } from "../components/SettlementPanel";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { HistoryIcon } from "../components/icons/history";
import { CircleDollarSignIcon } from "../components/icons/circle-dollar-sign";
import { CreditCardIcon } from "../components/icons/credit-card";
import { FileTextIcon } from "../components/icons/file-text";
import { SquarePenIcon } from "../components/icons/square-pen";
import { treeRows } from "../lib/categoryTree";
import { formatMoney } from "../lib/decimal";
import { STATUS_LABELS, entryStatus, todayIso, type InstallmentStatus } from "../lib/settlement";
import { formatDate } from "../lib/entryInstallments";
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

const PAGE_TEXT = {
  "": {
    title: "Lançamentos",
    description: "O que a empresa tem a receber e a pagar, dividido em parcelas.",
    create: "Novo lançamento",
  },
  receivable: {
    title: "Contas a receber",
    description: "O que os clientes ainda vão pagar, dividido em parcelas.",
    create: "Nova receita",
  },
  payable: {
    title: "Contas a pagar",
    description: "O que a empresa ainda vai pagar, dividido em parcelas.",
    create: "Nova despesa",
  },
} as const;

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function EntriesPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  // "Contas a receber" and "Contas a pagar" are this list with the kind in the address, as people expect to find them.
  const [params, setParams] = useSearchParams();
  const tipo = params.get("tipo");
  const pageKind: EntryKind | "" = tipo === "receivable" || tipo === "payable" ? tipo : "";
  const [typedFilters, setFilters] = useState<EntryFilters>(NO_ENTRY_FILTERS);
  const filters: EntryFilters = { ...typedFilters, kind: pageKind };
  const [editing, setEditing] = useState<Entry | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);
  const [settlementsOpenFor, setSettlementsOpenFor] = useState<string | null>(null);
  const [attachmentsOpenFor, setAttachmentsOpenFor] = useState<string | null>(null);
  const [chargesOpenFor, setChargesOpenFor] = useState<string | null>(null);
  const { banks, methods, categories, costCenters, parties, names, error: targetsError, loaded: targetsLoaded } = useEntryTargets(companyId);

  const {
    items: entries,
    setItems,
    nextCursor,
    loading,
    refreshing,
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
  // The kind comes from the address, so "Limpar filtros" could not clear it: it is not counted here.
  const isFiltered = applied.q !== "" || applied.category !== "";

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

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // An entry created from the top bar or from here: read the list again and flash the new row.
  useOnCreated("entry", (created) => {
    // What an earlier change said is not about this one.
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

  async function handleSubmit(input: EntryInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updateEntry(companyId, editing.id, input);
      setEditing(null);
      setSaved(`${updated.description} atualizado.`);
      // Read again, not changed in place: a new date, kind or category can move the entry to
      // another place in the list, or out of the filter the list is showing.
      await reload();
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <PageHeader title={PAGE_TEXT[pageKind].title} description={PAGE_TEXT[pageKind].description} />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("entry", pageKind === "" ? {} : { kind: pageKind })}>
            {PAGE_TEXT[pageKind].create}
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
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
                onChange={(event) => setFilters({ ...typedFilters, q: event.target.value })}
              />
            </label>
            <label>
              Mostrar
              <select
                value={filters.kind}
                onChange={(event) => {
                  // The kind lives in the address, so that a reload or a shared link shows the same list.
                  const next = new URLSearchParams(params);
                  if (event.target.value === "") next.delete("tipo");
                  else next.set("tipo", event.target.value);
                  setParams(next, { replace: true });
                }}
              >
                <option value="">Todos</option>
                <option value="receivable">Só a receber</option>
                <option value="payable">Só a pagar</option>
              </select>
            </label>
            <label>
              Categoria
              <select
                value={filters.category}
                onChange={(event) => setFilters({ ...typedFilters, category: event.target.value })}
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
              Ordem por competência
              <select
                value={filters.sort}
                onChange={(event) =>
                  setFilters({ ...typedFilters, sort: event.target.value as EntryFilters["sort"] })
                }
              >
                <option value="-competence_date">Mais recentes</option>
                <option value="competence_date">Mais antigas</option>
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
                      <ListRow index={index} highlighted={entry.id === highlightedId} tone={status === "overdue" && !entry.voided ? "critical" : undefined}>
                        <td>
                          <div className="cell-title">{entry.description}</div>
                          <div className="muted">
                            {entry.voided ? (
                              <span className="badge badge-dot badge-critical">Cancelado</span>
                            ) : (
                              <span className={STATUS_BADGES[status]}>{STATUS_LABELS[status]}</span>
                            )}{" "}
                            {entry.kind === "receivable" ? "A receber" : "A pagar"} ·{" "}
                            {names.get(entry.category_id) ?? ""}
                            {entry.recurrence_id !== null && " · Recorrente"}
                            {entry.sale_id !== null && " · De uma venda"}
                            {entry.sale_return_id !== null && " · Devolução"}
                          </div>
                        </td>
                        <td className="tabular">{formatDate(entry.competence_date)}</td>
                        <td className="tabular">{describeDue(entry)}</td>
                        <td className="num">{formatMoney(entry.total)}</td>
                        <td className="row-actions">
                          {canWrite && !entry.voided && entry.sale_id === null && entry.sale_return_id === null && (
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
                          {!entry.voided && (
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
                          )}
                          {!entry.voided && entry.kind === "receivable" && (
                            <Button
                              iconOnly
                              icon={CreditCardIcon}
                              type="button"
                              variant="ghost"
                              aria-label={`Cobranças de ${entry.description}`}
                              aria-expanded={chargesOpenFor === entry.id}
                              aria-controls={`charges-${entry.id}`}
                              onClick={() =>
                                setChargesOpenFor(chargesOpenFor === entry.id ? null : entry.id)
                              }
                            >
                              Cobranças
                            </Button>
                          )}
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
                      {chargesOpenFor === entry.id && (
                        <tr id={`charges-${entry.id}`}>
                          <td colSpan={5} role="region" aria-label={`Cobranças de ${entry.description}`}>
                            <ChargesPanel
                              // Remounted when the entry changes (a settlement moves the balances).
                              key={JSON.stringify(entry.installments)}
                              companyId={companyId}
                              entry={entry}
                              canWrite={canWrite}
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
          {!loading && entries.length > 0 && (
            <ListFooter
              count={entries.length}
              noun={["lançamento", "lançamentos"]}
              hasMore={nextCursor !== null}
              loadingMore={loadingMore}
              refreshing={refreshing}
              onLoadMore={() => void loadMore()}
            />
          )}
        </section>
        {canWrite && (
          <Sheet open={editing !== null} locked={saving} title={`Alterar ${shown?.description ?? ""}`} onClose={closeSheet}>
            {shown !== null &&
              (targetsLoaded ? (
                <EntryForm
                  key={shown.id}
                  entry={shown}
                  targets={{ categories, costCenters, parties }}
                  pending={saving}
                  error={formError}
                  onSubmit={(input) => void handleSubmit(input)}
                  onCancel={closeSheet}
                />
              ) : targetsError === null ? (
                <p role="status" className="status">
                  <Spinner /> Carregando…
                </p>
              ) : (
                <p role="alert">{targetsError}</p>
              ))}
          </Sheet>
        )}
      </div>
    </>
  );
}
