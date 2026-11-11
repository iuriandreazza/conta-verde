import { Fragment, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_PAYMENT_METHOD_FILTERS,
  api,
  errorMessage,
  type PaymentMethod,
  type PaymentMethodFilters,
  type PaymentMethodInput,
} from "../api";
import { Button } from "../components/Button";
import { ListRow } from "../components/ListRow";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { useLastValue } from "../lib/useLastValue";
import { PaymentMethodForm } from "../components/PaymentMethodForm";
import { PageHeader } from "../components/PageHeader";
import { Notice } from "../components/Notice";
import { PaymentMethodHistory } from "../components/PaymentMethodHistory";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { PAYMENT_METHOD_KINDS, paymentMethodKindLabel } from "../lib/kinds";
import { usePagedList } from "../lib/usePagedList";
import { useCanWrite } from "../lib/useCanWrite";
import { SquarePenIcon } from "../components/icons/square-pen";
import { StatusBadge } from "../components/StatusBadge";
import { useT } from "../i18n";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function PaymentMethodsPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [filters, setFilters] = useState<PaymentMethodFilters>(NO_PAYMENT_METHOD_FILTERS);
  const [editing, setEditing] = useState<PaymentMethod | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);

  const {
    items: methods,
    setItems,
    nextCursor,
    loading,
    loadingMore,
    error,
    applied,
    reload,
    loadMore,
  } = usePagedList<PaymentMethod, PaymentMethodFilters>(
    (applied, after) => api.listPaymentMethods(companyId, applied, after),
    filters,
  );
  const isFiltered = applied.q !== "" || applied.kind !== "" || applied.active !== "";

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A record made from the top bar or from here: read the list again and flash the new row.
  useOnCreated("paymentMethod", (created) => {
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

  async function handleSubmit(input: PaymentMethodInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updatePaymentMethod(companyId, editing.id, input);
      // In place: the list keeps its page and its filters.
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setEditing(null);
      setSaved(t("Payment method {name} updated.", { name: updated.name }));
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <PageHeader title={t("Payment methods")} description={t("How the company pays and gets paid: Pix, boleto, cash, cards.")} />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("paymentMethod")}>
            {t("New payment method")}
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
      <section className="card list-card">
        <h2>{t("Registered methods")}</h2>
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
            {t("Show")}
            <select
              value={filters.kind}
              onChange={(event) =>
                setFilters({ ...filters, kind: event.target.value as PaymentMethodFilters["kind"] })
              }
            >
              <option value="">{t("All methods")}</option>
              {PAYMENT_METHOD_KINDS.map((option) => (
                <option key={option.key} value={option.key}>
                  {t(option.label)}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Status")}
            <select
              value={filters.active}
              onChange={(event) =>
                setFilters({ ...filters, active: event.target.value as PaymentMethodFilters["active"] })
              }
            >
              <option value="">{t("Active and inactive methods")}</option>
              <option value="true">{t("Active methods only")}</option>
              <option value="false">{t("Inactive methods only")}</option>
            </select>
          </label>
          <label>
            {t("Order")}
            <select
              value={filters.sort}
              onChange={(event) =>
                setFilters({ ...filters, sort: event.target.value as PaymentMethodFilters["sort"] })
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
        ) : methods.length === 0 && error ? (
          <Button type="button" variant="ghost" onClick={() => void reload()}>
            {t("Try again")}
          </Button>
        ) : methods.length === 0 && isFiltered ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p role="status">{t("No records found.")}</p>
            <Button
              type="button"
              variant="ghost"
              onClick={() => setFilters({ ...NO_PAYMENT_METHOD_FILTERS, sort: filters.sort })}
            >
              {t("Clear filters")}
            </Button>
          </div>
        ) : methods.length === 0 ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p>{t("No payment methods yet.")}</p>
            <p className="muted">{t("Pix, boleto, cash, each card you accept…")}</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Name")}</th>
                  <th>{t("Type")}</th>
                  <th>{t("Status")}</th>
                  <th>
                    <span className="visually-hidden">{t("Actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {methods.map((method, index) => (
                  <Fragment key={method.id}>
                    <ListRow index={index} highlighted={method.id === highlightedId}>
                      <td>{method.name}</td>
                      <td>{paymentMethodKindLabel(method.kind, t)}</td>
                      <td><StatusBadge active={method.active} feminine /></td>
                      <td className="row-actions">
                        {canWrite && (
                          <Button
                            iconOnly
                            icon={SquarePenIcon}
                            type="button"
                            variant="ghost"
                            aria-label={t("Edit {name}", { name: method.name })}
                            disabled={saving}
                            onClick={() => {
                              setEditing(method);
                              setFormError(null);
                              setSaved(null);
                            }}
                          >
                            {t("Edit")}
                          </Button>
                        )}
                        <Button
                          iconOnly
                          type="button"
                          variant="ghost"
                          aria-label={t("History of {name}", { name: method.name })}
                          aria-expanded={historyOpenFor === method.id}
                          aria-controls={`history-${method.id}`}
                          icon={HistoryIcon}
                          onClick={() =>
                            setHistoryOpenFor(historyOpenFor === method.id ? null : method.id)
                          }
                        >
                          {t("History")}
                        </Button>
                      </td>
                    </ListRow>
                    {historyOpenFor === method.id && (
                      <tr id={`history-${method.id}`}>
                        <td colSpan={4} role="region" aria-label={t("History of {name}", { name: method.name })}>
                          <PaymentMethodHistory
                            // Remounted when the method changes, so an open history follows an edit.
                            key={JSON.stringify(method)}
                            companyId={companyId}
                            methodId={method.id}
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
              <PaymentMethodForm
                key={shown.id}
                method={shown}
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
