import { Fragment, useCallback, useEffect, useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_COMMISSION_RULE_FILTERS,
  NO_FILTERS,
  NO_PRODUCT_FILTERS,
  api,
  errorMessage,
  type CommissionRule,
  type CommissionRuleFilters,
  type CommissionRuleInput,
  type CommissionStatement,
  type Party,
  type Product,
} from "../api";
import { Button } from "../components/Button";
import { History } from "../components/History";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { HandCoinsIcon } from "../components/icons/hand-coins";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { SquarePenIcon } from "../components/icons/square-pen";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { CommissionRuleForm } from "../components/CommissionRuleForm";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { msg, useT, type Translate } from "../i18n";
import { useLastValue } from "../lib/useLastValue";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { StatusBadge } from "../components/StatusBadge";
import { formatDecimal, formatMoney } from "../lib/decimal";
import { formatDate, isIsoDate } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { todayIso } from "../lib/settlement";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";
import { HelpTitle } from "../components/HelpTitle";

const FIELD_LABELS: Record<string, string> = {
  seller_id: msg("Seller"),
  product_id: msg("Product"),
  rate: msg("Percentage"),
  starts_on: msg("Valid since"),
  ends_on: msg("Valid through"),
  active: msg("Status"),
};

const rate = (value: string) => `${formatDecimal(value)}%`;

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function CommissionsPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("sales");
  const [parties, setParties] = useState<Party[]>([]);
  const [products, setProducts] = useState<Product[]>([]);
  const [targetsError, setTargetsError] = useState<string | null>(null);
  const [targetsLoaded, setTargetsLoaded] = useState(false);

  useEffect(() => {
    let current = true;
    Promise.all([
      readUpTo((after) => api.listParties(companyId, NO_FILTERS, after)),
      readUpTo((after) => api.listProducts(companyId, NO_PRODUCT_FILTERS, after)),
    ])
      .then(([partyList, productList]) => {
        if (current) {
          setParties(partyList);
          setProducts(productList);
          setTargetsLoaded(true);
        }
      })
      .catch((failure: unknown) => current && setTargetsError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  const names = useMemo(() => new Map([...parties, ...products].map((item) => [item.id, item.name])), [parties, products]);
  const sellers = parties.filter((party) => party.is_seller);

  return (
    <>
      <PageHeader
        title={t("Commissions")}
        description={t("What each seller earned on completed sales, by the most specific rule that applied on the day of the sale.")}
      />
      <Notice message={targetsError} />
      <StatementCard companyId={companyId} sellers={sellers} names={names} />
      <RulesSection
        companyId={companyId}
        canWrite={canWrite}
        parties={parties}
        products={products}
        names={names}
        targetsLoaded={targetsLoaded}
        targetsError={targetsError}
      />
    </>
  );
}

function StatementCard({ companyId, sellers, names }: { companyId: string; sellers: Party[]; names: Map<string, string> }) {
  const t = useT();
  const today = todayIso();
  const [from, setFrom] = useState(`${today.slice(0, 8)}01`);
  const [to, setTo] = useState(today);
  const [seller, setSeller] = useState("");
  const [statement, setStatement] = useState<CommissionStatement | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  // The server orders sellers by id, which means nothing to a person.
  const sortedSellers = [...(statement?.sellers ?? [])].sort((a, b) =>
    (names.get(a.seller_id) ?? "").localeCompare(names.get(b.seller_id) ?? "", "pt-BR"),
  );

  const valid = isIsoDate(from) && isIsoDate(to) && from <= to;

  useEffect(() => {
    if (!valid) {
      setLoading(false);
      return;
    }
    let current = true;
    // What was on screen belongs to other dates or another seller: never show it under the new ones.
    setStatement(null);
    setLoading(true);
    setError(null);
    api
      .commissionStatement(companyId, from, to, seller)
      .then((loaded) => current && setStatement(loaded))
      .catch((failure: unknown) => current && setError(errorMessage(failure)))
      .finally(() => current && setLoading(false));
    return () => {
      current = false;
    };
  }, [companyId, from, to, seller, valid]);

  return (
    <section className="card list-card" aria-label={t("Commission statement")}>
      <HelpTitle level={2} title={t("Statement")}>
        <p>{t("The base is the net value of the items, less what came back in returns.")}</p>
      </HelpTitle>
      <div className="filters" role="search">
        <label>
          {t("From")}
          <input type="date" value={from} onChange={(event) => setFrom(event.target.value)} />
        </label>
        <label>
          {t("To")}
          <input type="date" value={to} onChange={(event) => setTo(event.target.value)} />
        </label>
        <label>
          {t("Seller")}
          <select value={seller} onChange={(event) => setSeller(event.target.value)}>
            <option value="">{t("Everyone")}</option>
            {sellers.map((party) => (
              <option key={party.id} value={party.id}>
                {party.name}
              </option>
            ))}
          </select>
        </label>
      </div>
      {!valid && <Notice message={t("Enter a period whose first day does not come after the last.")} />}
      <Notice message={error} />
      {loading && statement === null ? (
        <p role="status" className="status">
          <Spinner /> {t("Loading…")}
        </p>
      ) : statement !== null && valid && statement.sellers.length === 0 ? (
        <div className="empty">
          <HandCoinsIcon size={40} aria-hidden />
          <p role="status">{t("No commission in this period.")}</p>
          <p className="muted">{t("Only completed sales that have a seller count.")}</p>
        </div>
      ) : (
        statement !== null &&
        valid && (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Seller")}</th>
                  <th className="num">{t("Base")}</th>
                  <th className="num">{t("Commission")}</th>
                </tr>
              </thead>
              <tbody>
                {sortedSellers.map((entry) => (
                  <Fragment key={entry.seller_id}>
                    <tr>
                      <td className="cell-title">{names.get(entry.seller_id) ?? t("Seller")}</td>
                      <td className="num">{formatMoney(entry.base)}</td>
                      <td className="num">{formatMoney(entry.commission)}</td>
                    </tr>
                    {entry.sales.map((sale) => (
                      <tr key={sale.sale_id} className="muted">
                        <td>
                          {t("Sale {number} · {date} · {customer}", {
                            number: sale.number,
                            date: formatDate(sale.sold_on),
                            customer: names.get(sale.customer_id) ?? "",
                          })}
                        </td>
                        <td className="num">{formatMoney(sale.base)}</td>
                        <td className="num">{formatMoney(sale.commission)}</td>
                      </tr>
                    ))}
                  </Fragment>
                ))}
              </tbody>
              <tfoot>
                <tr>
                  <th scope="row">{t("Total")}</th>
                  <td className="num">{formatMoney(statement.base)}</td>
                  <td className="num">{formatMoney(statement.commission)}</td>
                </tr>
              </tfoot>
            </table>
          </div>
        )
      )}
    </section>
  );
}

interface RulesSectionProps {
  companyId: string;
  canWrite: boolean;
  parties: Party[];
  products: Product[];
  names: Map<string, string>;
  targetsLoaded: boolean;
  targetsError: string | null;
}

function RulesSection({ companyId, canWrite, parties, products, names, targetsLoaded, targetsError }: RulesSectionProps) {
  const t = useT();
  const [filters, setFilters] = useState<CommissionRuleFilters>(NO_COMMISSION_RULE_FILTERS);
  const [editing, setEditing] = useState<CommissionRule | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOf, setHistoryOf] = useState<string | null>(null);

  const { items, nextCursor, loading, loadingMore, error, applied, reload, loadMore } =
    usePagedList<CommissionRule, CommissionRuleFilters>((applied, after) => api.listCommissionRules(companyId, applied, after), filters);
  const isFiltered = applied.seller !== "" || applied.product !== "" || applied.active !== "";
  const sellers = parties.filter((party) => party.is_seller);
  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // A rule made from the top bar or from here: read the list again and flash the new row.
  useOnCreated("commissionRule", (created) => {
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

  async function handleSubmit(input: CommissionRuleInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updateCommissionRule(companyId, editing.id, input);
      setHighlightedId(updated.id);
      setEditing(null);
      setSaved(t("Rule of {rate} updated. Sales already made are recalculated in the statement.", { rate: rate(updated.rate) }));
      // The new dates or situation may move it in the list or out of a filter.
      await reload();
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  const historyLoader = useCallback((id: string) => () => api.commissionRuleHistory(companyId, id), [companyId]);

  return (
    <>
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("commissionRule")}>
            {t("New commission rule")}
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <section className="card list-card" aria-label={t("Commission rules")}>
        <h2>{t("Rules")}</h2>
        <div className="filters" role="search">
          <label>
            {t("Seller")}
            <select value={filters.seller} onChange={(event) => setFilters({ ...filters, seller: event.target.value })}>
              <option value="">{t("Everyone")}</option>
              {sellers.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Status")}
            <select
              value={filters.active}
              onChange={(event) => setFilters({ ...filters, active: event.target.value as CommissionRuleFilters["active"] })}
            >
              <option value="">{t("All")}</option>
              <option value="true">{t("Active rules")}</option>
              <option value="false">{t("Inactive rules")}</option>
            </select>
          </label>
        </div>
        <Notice message={error} />
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
            <HandCoinsIcon size={40} aria-hidden />
            <p role="status">{t("No rules found.")}</p>
            <Button type="button" variant="ghost" onClick={() => setFilters({ ...NO_COMMISSION_RULE_FILTERS })}>
              {t("Clear filters")}
            </Button>
          </div>
        ) : items.length === 0 ? (
          <div className="empty">
            <HandCoinsIcon size={40} aria-hidden />
            <p>{t("No rules yet.")}</p>
            <p className="muted">{t("Without a rule, nobody earns commission. Start with a general rule and add exceptions.")}</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Applies to")}</th>
                  <th className="num">{t("Percentage")}</th>
                  <th>{t("Period")}</th>
                  <th>{t("Status")}</th>
                  <th>
                    <span className="visually-hidden">{t("Actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {items.map((item, index) => (
                  <Fragment key={item.id}>
                    <ListRow index={index} highlighted={item.id === highlightedId}>
                      <td>
                        <div className="cell-title">{item.seller_id === null ? t("All sellers") : (names.get(item.seller_id) ?? t("Seller"))}</div>
                        <div className="muted">{item.product_id === null ? t("All products") : (names.get(item.product_id) ?? t("Product"))}</div>
                      </td>
                      <td className="num">{rate(item.rate)}</td>
                      <td>
                        {t("since {date}", { date: formatDate(item.starts_on) })}
                        {item.ends_on !== null && <div className="muted">{t("until {date}", { date: formatDate(item.ends_on) })}</div>}
                      </td>
                      <td>
                        <StatusBadge active={item.active} feminine />
                      </td>
                      <td className="row-actions">
                        {canWrite && (
                          <Button
                            iconOnly
                            icon={SquarePenIcon}
                            type="button"
                            variant="ghost"
                            aria-label={t("Change the rule of {rate} since {date}", { rate: rate(item.rate), date: formatDate(item.starts_on) })}
                            disabled={saving}
                            onClick={() => {
                              setEditing(item);
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
                          aria-label={t("History of the rule of {rate} since {date}", { rate: rate(item.rate), date: formatDate(item.starts_on) })}
                          aria-expanded={historyOf === item.id}
                          icon={HistoryIcon}
                          onClick={() => setHistoryOf(historyOf === item.id ? null : item.id)}
                        >
                          {t("History")}
                        </Button>
                      </td>
                    </ListRow>
                    {historyOf === item.id && (
                      <tr>
                        <td colSpan={5} role="region" aria-label={t("History of the rule of {rate}", { rate: rate(item.rate) })}>
                          <History
                            load={historyLoader(item.id)}
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
        <Sheet open={editing !== null} locked={saving} title={t("Change rule")} onClose={closeSheet}>
          {shown !== null &&
            (targetsLoaded ? (
              <CommissionRuleForm
                key={shown.id}
                rule={shown}
                parties={parties}
                products={products}
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
    </>
  );
}

/** A value of the trail as a person reads it: names for references, words for flags. */
function displayValue(field: string, value: string | null, names: Map<string, string>, t: Translate): string {
  if (value === null) return field === "seller_id" ? t("All sellers") : field === "product_id" ? t("All products") : "—";
  if (field === "seller_id" || field === "product_id") return names.get(value) ?? value;
  if (field === "rate") return rate(value);
  if (field === "starts_on" || field === "ends_on") return formatDate(value);
  if (field === "active") return value === "true" ? t("Enabled") : t("Disabled");
  return value;
}
