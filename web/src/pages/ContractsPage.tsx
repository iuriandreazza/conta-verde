import { Fragment, useCallback, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_CONTRACT_FILTERS,
  api,
  errorMessage,
  type BillingCycle,
  type Contract,
  type ContractFilters,
  type ContractInput,
  type ContractStatus,
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
import { ContractForm } from "../components/ContractForm";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { msg, useT, type Translate } from "../i18n";
import { useSalesTargets } from "../lib/useSalesTargets";
import { useLastValue } from "../lib/useLastValue";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { CONTRACT_STATUS_LABELS, CYCLE_LABELS } from "../lib/contractCycle";
import { formatMoney, isAmbiguousDecimal, toApiDecimal } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { useCanWrite } from "../lib/useCanWrite";
import { linesTrailText } from "../lib/trail";
import { usePagedList } from "../lib/usePagedList";
import { InfoTip } from "../components/InfoTip";

const FIELD_LABELS: Record<string, string> = {
  name: msg("Name"),
  customer_id: msg("Customer"),
  seller_id: msg("Seller"),
  payment_term_id: msg("Payment term"),
  category_id: msg("Category"),
  cost_center_id: msg("Cost center"),
  notes: msg("Notes"),
  lines: msg("Items"),
  cycle: msg("Cycle"),
  starts_on: msg("First charge"),
  ends_on: msg("Last charge"),
  status: msg("Status"),
};

type Panel = { kind: "adjust" | "terminate" | "history"; id: string };

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function ContractsPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("sales");
  const [filters, setFilters] = useState<ContractFilters>(NO_CONTRACT_FILTERS);
  const [editing, setEditing] = useState<Contract | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [panel, setPanel] = useState<Panel | null>(null);
  const [acting, setActing] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const targets = useSalesTargets(companyId);
  const { names, error: targetsError, loaded: targetsLoaded } = targets;

  const { items, setItems, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<Contract, ContractFilters>((applied, after) => api.listContracts(companyId, applied, after), filters);
  const isFiltered = applied.q !== "" || applied.status !== "";

  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A contract made from the top bar or from here: read the list again and flash the new row.
  useOnCreated("contract", (created) => {
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

  async function handleSubmit(input: ContractInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updateContract(companyId, editing.id, input);
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setEditing(null);
      setSaved(t("{name} updated. What was already charged does not change.", { name: updated.name }));
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
        setEditing(null);
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
        title={t("Contracts")}
        description={t("Recurring charges: each cycle becomes a completed sale and its receivable, on its own, on the day.")}
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("contract")}>
            {t("New contract")}
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
                value={filters.status}
                onChange={(event) => setFilters({ ...filters, status: event.target.value as ContractFilters["status"] })}
              >
                <option value="">{t("All")}</option>
                {Object.entries(CONTRACT_STATUS_LABELS).map(([key, label]) => (
                  <option key={key} value={key}>
                    {t(label)}
                  </option>
                ))}
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
              <p role="status">{t("No contracts found.")}</p>
              <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_CONTRACT_FILTERS })}>
                {t("Clear filters")}
              </Button>
            </div>
          ) : items.length === 0 ? (
            <div className="empty">
              <RotateCCWIcon size={40} aria-hidden />
              <p>{t("No contracts yet.")}</p>
              <p className="muted">{t("A monthly fee, a maintenance plan, a subscription: describe it once and it bills itself.")}</p>
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{t("Contract")}</th>
                    <th>{t("Cycle")}</th>
                    <th className="num">{t("Each cycle")}</th>
                    <th>{t("Status")}</th>
                    <th>
                      <span className="visually-hidden">{t("Actions")}</span>
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
                          {t(CYCLE_LABELS[contract.cycle])}
                          <div className="muted">
                            {t("since {date}", { date: formatDate(contract.starts_on) })}
                            {contract.ends_on !== null && ` ${t("until {date}", { date: formatDate(contract.ends_on) })}`}
                          </div>
                        </td>
                        <td className="num">{formatMoney(contract.total)}</td>
                        <td>
                          <span className={statusClass(contract.status)}>{t(CONTRACT_STATUS_LABELS[contract.status])}</span>
                          {contract.billed_through !== null && (
                            <div className="muted">{t("billed through {date}", { date: formatDate(contract.billed_through) })}</div>
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
                                aria-label={t("Change {name}", { name: contract.name })}
                                disabled={saving || acting}
                                onClick={() => {
                                  setEditing(contract);
                                  setFormError(null);
                                  setSaved(null);
                                }}
                              >
                                {t("Change")}
                              </Button>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={t("Adjust {name}", { name: contract.name })}
                                aria-expanded={panel?.kind === "adjust" && panel.id === contract.id}
                                onClick={() => toggle("adjust", contract.id)}
                              >
                                {t("Adjust")}
                              </Button>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={t("Terminate {name}", { name: contract.name })}
                                aria-expanded={panel?.kind === "terminate" && panel.id === contract.id}
                                onClick={() => toggle("terminate", contract.id)}
                              >
                                {t("Terminate")}
                              </Button>
                            </>
                          )}
                          <Button
                            iconOnly
                            type="button"
                            variant="ghost"
                            aria-label={t("History of {name}", { name: contract.name })}
                            aria-expanded={panel?.kind === "history" && panel.id === contract.id}
                            icon={HistoryIcon}
                            onClick={() => toggle("history", contract.id)}
                          >
                            {t("History")}
                          </Button>
                        </td>
                      </ListRow>
                      {panel?.id === contract.id && panel.kind === "adjust" && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={t("Adjustment of {name}", { name: contract.name })}>
                            <form
                              className="opening-form"
                              onSubmit={(event) => {
                                event.preventDefault();
                                const typed = String(new FormData(event.currentTarget).get("percent") ?? "");
                                if (isAmbiguousDecimal(typed) || toApiDecimal(typed) === undefined) {
                                  setActionError(t("Enter the percentage, for example 5,5 or -3."));
                                  return;
                                }
                                void act(
                                  () => api.adjustContract(companyId, contract.id, toApiDecimal(typed) ?? ""),
                                  (done) => t("{name} adjusted: each cycle now charges {amount}, from the next one.", { name: done.name, amount: formatMoney(done.total) }),
                                );
                              }}
                            >
                              <label>
                                {t("Adjustment (%)")}
                                <input name="percent" inputMode="decimal" autoComplete="off" placeholder="5,5" required />
                              </label>
                              <div className="action-with-tip">
                                <Button type="submit" icon={CheckIcon} pending={acting}>
                                  {t("Apply adjustment")}
                                </Button>
                                <InfoTip label={t("About the adjustment")} align="start">
                                  <p>
                                    {t("Changes the price of each item (rounded to the cent) from the next cycle; what was already charged does not change.")}
                                  </p>
                                </InfoTip>
                              </div>
                              <Notice message={actionError} />
                            </form>
                          </td>
                        </tr>
                      )}
                      {panel?.id === contract.id && panel.kind === "terminate" && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={t("Termination of {name}", { name: contract.name })}>
                            <form
                              className="opening-form"
                              onSubmit={(event) => {
                                event.preventDefault();
                                const on = String(new FormData(event.currentTarget).get("on") ?? "");
                                if (!isIsoDate(on)) {
                                  setActionError(t("Enter the last day of billing."));
                                  return;
                                }
                                void act(
                                  () => api.terminateContract(companyId, contract.id, on),
                                  (done) => t("{name} terminated: nothing will be charged after {date}.", { name: done.name, date: formatDate(on) }),
                                );
                              }}
                            >
                              <label>
                                {t("Last day of billing")}
                                <input name="on" type="date" required defaultValue={todayIso()} />
                              </label>
                              <Button type="submit" pending={acting}>
                                {t("Terminate contract")}
                              </Button>
                              <p className="muted">{t("Terminating cannot be undone: to bill again, register another contract.")}</p>
                              <Notice message={actionError} />
                            </form>
                          </td>
                        </tr>
                      )}
                      {panel?.id === contract.id && panel.kind === "history" && (
                        <tr>
                          <td colSpan={5} role="region" aria-label={t("History of {name}", { name: contract.name })}>
                            <History
                              load={historyLoader(contract.id)}
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
          <Sheet open={editing !== null} locked={saving} title={t("Change {name}", { name: shown?.name ?? "" })} onClose={closeSheet}>
            {shown !== null &&
              (targetsLoaded ? (
                <ContractForm
                  key={shown.id}
                  companyId={companyId}
                  contract={shown}
                  targets={targets}
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
function displayValue(field: string, value: string | null, names: Map<string, string>, t: Translate): string {
  if (value === null) return "—";
  if (["customer_id", "seller_id", "payment_term_id", "category_id", "cost_center_id"].includes(field)) {
    return names.get(value) ?? value;
  }
  if (field === "status") return t(CONTRACT_STATUS_LABELS[value as ContractStatus] ?? value);
  if (field === "cycle") return t(CYCLE_LABELS[value as BillingCycle] ?? value);
  if (field === "starts_on" || field === "ends_on") return formatDate(value);
  if (field === "lines") return linesTrailText(value, names, t);
  return value;
}
