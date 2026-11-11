import { Fragment, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_PAYMENT_TERM_FILTERS,
  api,
  errorMessage,
  type PaymentTerm,
  type PaymentTermFilters,
  type PaymentTermInput,
} from "../api";
import { Button } from "../components/Button";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PlusIcon } from "../components/icons/plus";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { useLastValue } from "../lib/useLastValue";
import { PageHeader } from "../components/PageHeader";
import { PaymentTermForm } from "../components/PaymentTermForm";
import { PaymentTermHistory } from "../components/PaymentTermHistory";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { describeInstallments } from "../lib/installments";
import { usePagedList } from "../lib/usePagedList";
import { useCanWrite } from "../lib/useCanWrite";
import { SquarePenIcon } from "../components/icons/square-pen";
import { StatusBadge } from "../components/StatusBadge";
import { useT } from "../i18n";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function PaymentTermsPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [filters, setFilters] = useState<PaymentTermFilters>(NO_PAYMENT_TERM_FILTERS);
  const [editing, setEditing] = useState<PaymentTerm | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);

  const {
    items: terms,
    setItems,
    nextCursor,
    loading,
    loadingMore,
    error,
    applied,
    reload,
    loadMore,
  } = usePagedList<PaymentTerm, PaymentTermFilters>(
    (applied, after) => api.listPaymentTerms(companyId, applied, after),
    filters,
  );
  const isFiltered = applied.q !== "" || applied.active !== "";

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A record made from the top bar or from here: read the list again and flash the new row.
  useOnCreated("paymentTerm", (created) => {
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

  async function handleSubmit(input: PaymentTermInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updatePaymentTerm(companyId, editing.id, input);
      // In place: the list keeps its page and its filters.
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setEditing(null);
      setSaved(t("{name} updated.", { name: updated.name }));
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <PageHeader
        title={t("Payment terms")}
        description={t("How an amount is spread over time: upfront, 30/60/90, down payment and balance.")}
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("paymentTerm")}>
            {t("New payment term")}
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
        <section className="card list-card">
          <h2>{t("Registered")}</h2>
          <div className="filters" role="search">
            <label>
              {t("Search")}
              <input
                type="search"
                value={filters.q}
                placeholder={t("Name")}
                autoComplete="off"
                maxLength={100}
                onChange={(event) => setFilters({ ...filters, q: event.target.value })}
              />
            </label>
            <label>
              {t("Status")}
              <select
                value={filters.active}
                onChange={(event) =>
                  setFilters({ ...filters, active: event.target.value as PaymentTermFilters["active"] })
                }
              >
                <option value="">{t("Active and inactive records")}</option>
                <option value="true">{t("Active only")}</option>
                <option value="false">{t("Inactive only")}</option>
              </select>
            </label>
            <label>
              {t("Sort order")}
              <select
                value={filters.sort}
                onChange={(event) =>
                  setFilters({ ...filters, sort: event.target.value as PaymentTermFilters["sort"] })
                }
              >
                <option value="name">{t("Name (A–Z)")}</option>
                <option value="-name">{t("Name (Z–A)")}</option>
              </select>
            </label>
          </div>
          <Notice message={error} />
          {loading ? (
            <p role="status" className="status">
              <Spinner /> {t("Loading…")}
            </p>
          ) : terms.length === 0 && error ? (
            <Button type="button" variant="ghost" onClick={() => void reload()}>
              {t("Try again")}
            </Button>
          ) : terms.length === 0 && isFiltered ? (
            <div className="empty">
              <BriefcaseBusinessIcon size={40} aria-hidden />
              <p role="status">{t("No records found.")}</p>
              <Button
                type="button"
                variant="ghost"
                onClick={() => setFilters({ ...NO_PAYMENT_TERM_FILTERS, sort: filters.sort })}
              >
                {t("Clear filters")}
              </Button>
            </div>
          ) : terms.length === 0 ? (
            <div className="empty">
              <BriefcaseBusinessIcon size={40} aria-hidden />
              <p>{t("No payment terms yet.")}</p>
              <p className="muted">{t("Upfront, 30 days, 30/60/90, 50% down payment and 50% in 30 days…")}</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{t("Name")}</th>
                    <th>{t("Installments")}</th>
                    <th>{t("Status")}</th>
                    <th>
                      <span className="visually-hidden">{t("Actions")}</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {terms.map((term, index) => (
                    <Fragment key={term.id}>
                      <ListRow index={index} highlighted={term.id === highlightedId}>
                        <td>{term.name}</td>
                        <td>{describeInstallments(term.installments, t)}</td>
                        <td><StatusBadge active={term.active} /></td>
                        <td className="row-actions">
                          {canWrite && (
                            <Button
                              iconOnly
                              icon={SquarePenIcon}
                              type="button"
                              variant="ghost"
                              aria-label={t("Edit {name}", { name: term.name })}
                              disabled={saving}
                              onClick={() => {
                                setEditing(term);
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
                            aria-label={t("History of {name}", { name: term.name })}
                            aria-expanded={historyOpenFor === term.id}
                            aria-controls={`history-${term.id}`}
                            icon={HistoryIcon}
                            onClick={() =>
                              setHistoryOpenFor(historyOpenFor === term.id ? null : term.id)
                            }
                          >
                            {t("History")}
                          </Button>
                        </td>
                      </ListRow>
                      {historyOpenFor === term.id && (
                        <tr id={`history-${term.id}`}>
                          <td colSpan={4} role="region" aria-label={t("History of {name}", { name: term.name })}>
                            <PaymentTermHistory
                              // Remounted when the term changes, so an open history follows an edit.
                              key={JSON.stringify(term)}
                              companyId={companyId}
                              termId={term.id}
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
            <Button
              type="button"
              variant="ghost"
              icon={ChevronDownIcon}
              pending={loadingMore}
              onClick={() => void loadMore()}
            >
              {t("Load more")}
            </Button>
          )}
        </section>
        {canWrite && (
          <Sheet open={editing !== null} locked={saving} title={t("Edit {name}", { name: shown?.name ?? "" })} onClose={closeSheet}>
            {shown !== null && (
              <PaymentTermForm
                key={shown.id}
                term={shown}
                pending={saving}
                error={formError}
                onSubmit={(input) => void handleSubmit(input)}
                onCancel={closeSheet}
              />
            )}
          </Sheet>
        )}
      </div>
    </>
  );
}
