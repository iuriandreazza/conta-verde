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
import { msg, useT, type Translate } from "../i18n";


const FIELD_LABELS: Record<string, string> = {
  kind: msg("Type"),
  description: msg("Description"),
  party_id: msg("Customer or supplier"),
  category_id: msg("Category"),
  cost_center_id: msg("Cost center"),
  amount: msg("Amount"),
  frequency: msg("Frequency"),
  starts_on: msg("First due date"),
  ends_on: msg("Last day"),
  active: msg("Status"),
};


// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function RecurrencesPage() {
  const t = useT();
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
      setSaved(t("{name} updated. What was already generated does not change.", { name: updated.description }));
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
        title={t("Recurrences")}
        description={t("Rent, subscriptions, monthly fees: what repeats. The entries for the next two months are generated on their own.")}
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("recurrence")}>
            {t("New recurrence")}
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
          <h2>{t("Set up")}</h2>
          <div className="filters" role="search">
            <label>
              {t("Search")}
              <input
                type="search"
                value={filters.q}
                placeholder={t("Description")}
                autoComplete="off"
                maxLength={100}
                onChange={(event) => setFilters({ ...filters, q: event.target.value })}
              />
            </label>
            <label>
              {t("Show")}
              <select
                value={filters.kind}
                onChange={(event) => setFilters({ ...filters, kind: event.target.value as RecurrenceFilters["kind"] })}
              >
                <option value="">{t("Receivable and payable")}</option>
                <option value="receivable">{t("Receivables only")}</option>
                <option value="payable">{t("Payables only")}</option>
              </select>
            </label>
            <label>
              {t("Status")}
              <select
                value={filters.active}
                onChange={(event) => setFilters({ ...filters, active: event.target.value as RecurrenceFilters["active"] })}
              >
                <option value="">{t("Running and paused")}</option>
                <option value="true">{t("Only running")}</option>
                <option value="false">{t("Only paused")}</option>
              </select>
            </label>
          </div>
          <Notice message={error ?? targetsError} />
          {loading ? (
            <p role="status" className="status">
              <Spinner /> {t("Loading…")}
            </p>
          ) : items.length === 0 && error ? (
            <Button type="button" variant="ghost" onClick={() => void reload()}>
              {t("Try again")}
            </Button>
          ) : items.length === 0 && isFiltered ? (
            <div className="empty">
              <RotateCCWIcon size={40} aria-hidden />
              <p role="status">{t("No recurrence found.")}</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_RECURRENCE_FILTERS })}>
                {t("Clear filters")}
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <RotateCCWIcon size={40} aria-hidden />
              <p>{t("No recurrences yet.")}</p>
              <p className="muted">{t("The rent, the system subscription, a customer's monthly fee.")}</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{t("Description")}</th>
                    <th>{t("When")}</th>
                    <th className="num">{t("Amount")}</th>
                    <th>{t("Status")}</th>
                    <th>
                      <span className="visually-hidden">{t("Actions")}</span>
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
                            {recurrence.kind === "receivable" ? t("Receivable") : t("Payable")} ·{" "}
                            {names.get(recurrence.category_id) ?? ""}
                          </div>
                        </td>
                        <td>
                          {frequencyLabel(recurrence.frequency, t)}
                          <div className="muted">
                            {t("since {date}", { date: formatDate(recurrence.starts_on) })}
                            {recurrence.ends_on !== null && ` ${t("until {date}", { date: formatDate(recurrence.ends_on) })}`}
                          </div>
                        </td>
                        <td className="num">{formatMoney(recurrence.amount)}</td>
                        <td>
                          <StatusBadge active={recurrence.active} feminine activeLabel={t("Running")} inactiveLabel={t("Recurrence paused")} />
                        </td>
                        <td className="row-actions">
                          {canWrite && (
                            <Button
                              iconOnly
                              icon={SquarePenIcon}
                              type="button"
                              variant="ghost"
                              aria-label={t("Edit {name}", { name: recurrence.description })}
                              disabled={saving}
                              onClick={() => {
                                setEditing(recurrence);
                                setFormError(null);
                                setSaved(null);
                              }}
                            >
                              {t("Change")}
                            </Button>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={t("History of {name}", { name: recurrence.description })}
                            aria-expanded={historyOpenFor === recurrence.id}
                            aria-controls={`history-${recurrence.id}`}
                            icon={HistoryIcon}
                            onClick={() => setHistoryOpenFor(historyOpenFor === recurrence.id ? null : recurrence.id)}
                          >
                            {t("History")}
                          </Button>
                        </td>
                      </ListRow>
                      {historyOpenFor === recurrence.id && (
                        <tr id={`history-${recurrence.id}`}>
                          <td colSpan={5} role="region" aria-label={t("History of {name}", { name: recurrence.description })}>
                            <History
                              load={historyLoader(recurrence.id)}
                              fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)}
                              valueLabel={(field, value) => displayValue(field, value, names, t)}
                              emptyMessage={t("No records.")}
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
              {t("Load more")}
            </Button>
          )}
        </section>
        {canWrite && (
          <Sheet open={editing !== null} locked={saving} title={t("Edit {name}", { name: shown?.description ?? "" })} onClose={closeSheet}>
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

/** A value of the trail as a person reads it: names for references, words for keys, `R$` for the amount. */
function displayValue(field: string, value: string | null, names: Map<string, string>, t: Translate): string {
  if (value === null) return "—";
  if (["party_id", "category_id", "cost_center_id"].includes(field)) return names.get(value) ?? value;
  if (field === "kind") return value === "receivable" ? t("Receivable") : t("Payable");
  if (field === "frequency") return frequencyLabel(value as RecurrenceFrequency, t);
  if (field === "amount") return formatMoney(value);
  if (field === "starts_on" || field === "ends_on") return formatDate(value);
  if (field === "active") return value === "true" ? t("Running") : t("Recurrence paused");
  return value;
}
