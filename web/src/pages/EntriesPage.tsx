import { Fragment, useState } from "react";
import { useParams, useSearchParams } from "react-router-dom";
import {
  NO_ENTRY_FILTERS,
  api,
  errorMessage,
  type Entry,
  type EntryFilters,
  type EntryInput,
} from "../api";
import { entryKindOf } from "../lib/entryKind";
import { PlusIcon } from "../components/icons/plus";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { useLastValue } from "../lib/useLastValue";
import { useEntryTargets } from "../lib/useEntryTargets";
import { ListFooter } from "../components/ListFooter";
import { Button } from "../components/Button";
import { CsvImport } from "../components/CsvImport";
import { EntryForm } from "../components/EntryForm";
import { EntryHistory } from "../components/EntryHistory";
import { ListTools } from "../components/ListTools";
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
import { formatMoney, toInputDecimal } from "../lib/decimal";
import { STATUS_LABELS, entryStatus, todayIso, type InstallmentStatus } from "../lib/settlement";
import { formatDate } from "../lib/entryInstallments";
import { usePagedList } from "../lib/usePagedList";
import { useCanWrite } from "../lib/useCanWrite";
import { importEntriesInPieces } from "../lib/entryImport";
import { msg, useI18n, type Translate } from "../i18n";

const STATUS_BADGES: Record<InstallmentStatus, string> = {
  open: "badge badge-dot badge-muted",
  partial: "badge badge-dot badge-warning",
  settled: "badge badge-dot badge-ok",
  overdue: "badge badge-dot badge-critical",
};

function describeDue(entry: Entry, t: Translate): string {
  const [first] = entry.installments;
  const last = entry.installments[entry.installments.length - 1];
  return entry.installments.length === 1
    ? formatDate(first.due_date)
    : t("{count} installments, from {from} to {to}", {
        count: entry.installments.length,
        from: formatDate(first.due_date),
        to: formatDate(last.due_date),
      });
}

const ENTRY_COLUMNS = [
  msg("Type"),
  msg("Description"),
  msg("Customer or supplier"),
  msg("Category"),
  msg("Accrual date"),
  msg("Installments"),
  msg("First due date"),
  msg("Total"),
  msg("Status"),
];

function entryRow(entry: Entry, names: Map<string, string>, t: Translate): string[] {
  return [
    entry.kind === "receivable" ? t("Receivable") : t("Payable"),
    entry.description,
    entry.party_id === null ? "" : (names.get(entry.party_id) ?? ""),
    names.get(entry.category_id) ?? "",
    formatDate(entry.competence_date),
    String(entry.installments.length),
    formatDate(entry.installments[0].due_date),
    toInputDecimal(entry.total),
    entry.voided ? t("Cancelled") : "",
  ];
}

const PAGE_TEXT = {
  "": {
    title: msg("Entries"),
    description: msg("What the company has to receive and to pay, split into installments."),
    create: msg("New entry"),
    file: "lancamentos",
  },
  receivable: {
    title: msg("Receivables"),
    description: msg("What customers are yet to pay, split into installments."),
    create: msg("New revenue"),
    file: "contas-a-receber",
  },
  payable: {
    title: msg("Payables"),
    description: msg("What the company is yet to pay, split into installments."),
    create: msg("New expense"),
    file: "contas-a-pagar",
  },
} as const;

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function EntriesPage() {
  const { t, rich } = useI18n();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  // "Contas a receber" and "Contas a pagar" are this list with the kind in the address, as people expect to find them.
  const [params, setParams] = useSearchParams();
  const pageKind = entryKindOf(params.toString());
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
  // Layout keeps this page mounted when only `?tipo=` changes, so what belongs to the other list is dropped here.
  const [shownKind, setShownKind] = useState(pageKind);
  if (shownKind !== pageKind) {
    setShownKind(pageKind);
    setSaved(null);
    setHighlightedId(null);
    setHistoryOpenFor(null);
    setSettlementsOpenFor(null);
    setAttachmentsOpenFor(null);
    setChargesOpenFor(null);
  }
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
      setSaved(t("{name} updated.", { name: updated.description }));
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
      <PageHeader title={t(PAGE_TEXT[pageKind].title)} description={t(PAGE_TEXT[pageKind].description)} />
      <div className="list-actions">
        {canWrite && (
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("entry", pageKind === "" ? {} : { kind: pageKind })}>
            {t(PAGE_TEXT[pageKind].create)}
          </Button>
        )}
        <ListTools
          filename={PAGE_TEXT[pageKind].file}
          header={ENTRY_COLUMNS.map((column) => t(column))}
          fetchPage={(after) => api.listEntries(companyId, applied, after)}
          toRow={(entry) => entryRow(entry, names, t)}
        />
      </div>
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
          <h2>{t("Recorded")}</h2>
          <div className="filters" role="search">
            <label>
              {t("Search")}
              <input
                type="search"
                value={filters.q}
                placeholder={t("Description")}
                autoComplete="off"
                maxLength={100}
                onChange={(event) => setFilters({ ...typedFilters, q: event.target.value })}
              />
            </label>
            <label>
              {t("Show")}
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
                <option value="">{t("All")}</option>
                <option value="receivable">{t("Receivables only")}</option>
                <option value="payable">{t("Payables only")}</option>
              </select>
            </label>
            <label>
              {t("Category")}
              <select
                value={filters.category}
                onChange={(event) => setFilters({ ...typedFilters, category: event.target.value })}
              >
                <option value="">{t("All categories")}</option>
                {treeRows(categories).map(({ category, depth }) => (
                  <option key={category.id} value={category.id}>
                    {"— ".repeat(depth)}
                    {category.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("Order by accrual date")}
              <select
                value={filters.sort}
                onChange={(event) =>
                  setFilters({ ...typedFilters, sort: event.target.value as EntryFilters["sort"] })
                }
              >
                <option value="-competence_date">{t("Newest")}</option>
                <option value="competence_date">{t("Oldest")}</option>
              </select>
            </label>
          </div>
          <Notice message={error ?? targetsError} />
          {loading ? (
            <p role="status" className="status">
              <Spinner /> {t("Loading…")}
            </p>
          ) : entries.length === 0 && error ? (
            <Button type="button" variant="ghost" onClick={() => void reload()}>
              {t("Try again")}
            </Button>
          ) : entries.length === 0 && isFiltered ? (
            <div className="empty">
              <BriefcaseBusinessIcon size={40} aria-hidden />
              <p role="status">{t("No entry found.")}</p>
              <Button
                type="button"
                variant="ghost"
                onClick={() => setFilters({ ...NO_ENTRY_FILTERS, sort: filters.sort })}
              >
                {t("Clear filters")}
              </Button>
            </div>
          ) : entries.length === 0 ? (
            <div className="empty">
              <BriefcaseBusinessIcon size={40} aria-hidden />
              <p>{t("No entries yet.")}</p>
              <p className="muted">
                {t("A credit sale, the month's rent, an electricity bill: each one is an entry with its installments.")}
              </p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{t("Description")}</th>
                    <th>{t("Accrual date")}</th>
                    <th>{t("Due date")}</th>
                    <th className="num">{t("Total")}</th>
                    <th>
                      <span className="visually-hidden">{t("Actions")}</span>
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
                              <span className="badge badge-dot badge-critical">{t("Cancelled")}</span>
                            ) : (
                              <span className={STATUS_BADGES[status]}>{t(STATUS_LABELS[status])}</span>
                            )}{" "}
                            {entry.kind === "receivable" ? t("Receivable") : t("Payable")} ·{" "}
                            {names.get(entry.category_id) ?? ""}
                            {entry.recurrence_id !== null && ` · ${t("Recurring")}`}
                            {entry.sale_id !== null && ` · ${t("From a sale")}`}
                            {entry.sale_return_id !== null && ` · ${t("Return")}`}
                          </div>
                        </td>
                        <td className="tabular">{formatDate(entry.competence_date)}</td>
                        <td className="tabular">{describeDue(entry, t)}</td>
                        <td className="num">{formatMoney(entry.total)}</td>
                        <td className="row-actions">
                          {canWrite && !entry.voided && entry.sale_id === null && entry.sale_return_id === null && (
                            <Button
                              iconOnly
                              icon={SquarePenIcon}
                              type="button"
                              variant="ghost"
                              aria-label={t("Edit {name}", { name: entry.description })}
                              disabled={saving}
                              onClick={() => {
                                setEditing(entry);
                                setFormError(null);
                                setSaved(null);
                              }}
                            >
                              {t("Edit")}
                            </Button>
                          )}
                          {!entry.voided && (
                            <Button
                              iconOnly
                              icon={CircleDollarSignIcon}
                              type="button"
                              variant="ghost"
                              aria-label={t("Settlements of {name}", { name: entry.description })}
                              aria-expanded={settlementsOpenFor === entry.id}
                              aria-controls={`settlements-${entry.id}`}
                              onClick={() =>
                                setSettlementsOpenFor(settlementsOpenFor === entry.id ? null : entry.id)
                              }
                            >
                              {t("Settlements")}
                            </Button>
                          )}
                          {!entry.voided && entry.kind === "receivable" && (
                            <Button
                              iconOnly
                              icon={CreditCardIcon}
                              type="button"
                              variant="ghost"
                              aria-label={t("Charges of {name}", { name: entry.description })}
                              aria-expanded={chargesOpenFor === entry.id}
                              aria-controls={`charges-${entry.id}`}
                              onClick={() =>
                                setChargesOpenFor(chargesOpenFor === entry.id ? null : entry.id)
                              }
                            >
                              {t("Charges")}
                            </Button>
                          )}
                          <Button
                            iconOnly
                            icon={FileTextIcon}
                            type="button"
                            variant="ghost"
                            aria-label={t("Attachments of {name}", { name: entry.description })}
                            aria-expanded={attachmentsOpenFor === entry.id}
                            aria-controls={`attachments-${entry.id}`}
                            onClick={() =>
                              setAttachmentsOpenFor(attachmentsOpenFor === entry.id ? null : entry.id)
                            }
                          >
                            {t("Attachments")}
                          </Button>
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={t("History of {name}", { name: entry.description })}
                            aria-expanded={historyOpenFor === entry.id}
                            aria-controls={`history-${entry.id}`}
                            icon={HistoryIcon}
                            onClick={() =>
                              setHistoryOpenFor(historyOpenFor === entry.id ? null : entry.id)
                            }
                          >
                            {t("History")}
                          </Button>
                        </td>
                      </ListRow>
                      {settlementsOpenFor === entry.id && (
                        <tr id={`settlements-${entry.id}`}>
                          <td colSpan={5} role="region" aria-label={t("Settlements of {name}", { name: entry.description })}>
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
                          <td colSpan={5} role="region" aria-label={t("Charges of {name}", { name: entry.description })}>
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
                          <td colSpan={5} role="region" aria-label={t("Attachments of {name}", { name: entry.description })}>
                            <AttachmentsPanel companyId={companyId} entryId={entry.id} canWrite={canWrite} />
                          </td>
                        </tr>
                      )}
                      {historyOpenFor === entry.id && (
                        <tr id={`history-${entry.id}`}>
                          <td colSpan={5} role="region" aria-label={t("History of {name}", { name: entry.description })}>
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
              noun={[t("entry"), t("entries")]}
              hasMore={nextCursor !== null}
              loadingMore={loadingMore}
              refreshing={refreshing}
              onLoadMore={() => void loadMore()}
            />
          )}
        </section>
        {canWrite && (
          <CsvImport
            title={t("Import from a CSV file")}
            template={ENTRIES_TEMPLATE}
            templateName="lancamentos.csv"
            created={[t("entry created"), t("entries created")]}
            run={async (file, dryRun) =>
              importEntriesInPieces(
                await file.text(),
                (csv, dry) => api.importEntriesPiece(companyId, csv, dry),
                dryRun,
              )
            }
            onImported={() => void reload()}
          >
            <p>
              {rich(
                "Each row is an entry of one installment. The first row names the columns, in any order. Required: {reference} (the row's code in the file, which does not repeat: it is what stops it being imported twice), {kind} ({receivable} or {payable}), {description}, {category} (the path {path}, or just the name of a root), {competence_date} and {due_date} ({format}) and {amount} (with a decimal point).",
                {
                  reference: <code>reference</code>,
                  kind: <code>kind</code>,
                  receivable: <code>receivable</code>,
                  payable: <code>payable</code>,
                  description: <code>description</code>,
                  category: <code>category</code>,
                  path: <code>{t("Root > Child")}</code>,
                  competence_date: <code>competence_date</code>,
                  due_date: <code>due_date</code>,
                  format: <code>{t("YYYY-MM-DD")}</code>,
                  amount: <code>amount</code>,
                },
              )}
            </p>
            <p>
              {rich(
                "Optional: {party_document} and {party_name}, {cost_center}. For an entry that is already paid, {settled_on} and {bank_account}, and where it applies {applied}, {interest}, {fine} and {discount}. The category, cost center and account must be registered and active.",
                {
                  party_document: <code>party_document</code>,
                  party_name: <code>party_name</code>,
                  cost_center: <code>cost_center</code>,
                  settled_on: <code>settled_on</code>,
                  bank_account: <code>bank_account</code>,
                  applied: <code>applied</code>,
                  interest: <code>interest</code>,
                  fine: <code>fine</code>,
                  discount: <code>discount</code>,
                },
              )}
            </p>
            <p>
              {rich(
                "Nothing is imported while any row has a problem. A row already imported, by the same {reference}, is ignored. Large files are sent in parts, all checked before the first is written.",
                { reference: <code>reference</code> },
              )}
            </p>
          </CsvImport>
        )}
        {canWrite && (
          <Sheet open={editing !== null} locked={saving} title={t("Edit {name}", { name: shown?.description ?? "" })} onClose={closeSheet}>
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
                  <Spinner /> {t("Loading…")}
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

const ENTRIES_TEMPLATE = [
  "reference,kind,description,party_name,category,cost_center,competence_date,due_date,amount,settled_on,bank_account",
  "ex-1,payable,Aluguel de março,Imobiliária Central,Despesas > Aluguel,,2026-03-01,2026-03-10,2500.00,2026-03-10,Conta principal",
].join("\n");
