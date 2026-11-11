import { Fragment, useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import {
  NO_STATEMENT_LINE_FILTERS,
  api,
  errorMessage,
  type BankAccount,
  type Category,
  type LineSuggestion,
  type StatementLine,
  type StatementLineFilters,
} from "../api";
import { Button } from "../components/Button";
import { CsvImport } from "../components/CsvImport";
import { CheckIcon } from "../components/icons/check";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { RotateCCWIcon } from "../components/icons/rotate-ccw";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { treeRows } from "../lib/categoryTree";
import { formatMoney } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";
import { msg, useT } from "../i18n";

const STATUS_LABELS: Record<StatementLine["status"], string> = {
  pending: msg("Pending"),
  matched: msg("Matched"),
  ignored: msg("Ignored"),
};

const STATUS_BADGES: Record<StatementLine["status"], string> = {
  pending: "badge badge-dot badge-warning",
  matched: "badge badge-dot badge-ok",
  ignored: "badge badge-muted",
};

type Open = { id: string; mode: "suggestions" | "entry" } | null;

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: a reply still in flight for another company lands on an unmounted component.
export function ReconciliationPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [filters, setFilters] = useState<StatementLineFilters>(NO_STATEMENT_LINE_FILTERS);
  const [accounts, setAccounts] = useState<BankAccount[]>([]);
  const [categories, setCategories] = useState<Category[]>([]);
  const [targetsError, setTargetsError] = useState<string | null>(null);
  // The account the file is for: the filter may be "all of them", a file is for one.
  const [importAccount, setImportAccount] = useState("");
  const [open, setOpen] = useState<Open>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  // Bumped after every action, so that the suggestions open are asked for again: one just taken is gone.
  const [suggestionsVersion, setSuggestionsVersion] = useState(0);

  useEffect(() => {
    let current = true;
    Promise.all([
      readUpTo((after) => api.listBankAccounts(companyId, undefined, after)),
      api.listCategories(companyId),
    ])
      .then(([accountList, categoryList]) => {
        if (current) {
          setAccounts(accountList);
          setCategories(categoryList.items);
        }
      })
      .catch((failure: unknown) => current && setTargetsError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  const accountName = (id: string) => accounts.find((account) => account.id === id)?.name ?? "—";
  const activeAccounts = accounts.filter((account) => account.active);
  const categoryRows = treeRows(categories).filter(
    ({ category }) => category.active && category.dre_group !== "transfers",
  );

  const { items, nextCursor, loading, loadingMore, error, reload, loadMore } = usePagedList<
    StatementLine,
    StatementLineFilters
  >((applied, after) => api.listStatementLines(companyId, applied, after), filters);

  /** Runs one action on a line, then shows the line as it is now: it may leave the filter's list. */
  async function act(line: StatementLine, run: () => Promise<unknown>, done: string) {
    if (busyId !== null) {
      return;
    }
    setBusyId(line.id);
    setActionError(null);
    setNotice(null);
    try {
      await run();
      setOpen(null);
      setNotice(done);
    } catch (failure) {
      setActionError(errorMessage(failure));
    } finally {
      await reload();
      setSuggestionsVersion((version) => version + 1);
      setBusyId(null);
    }
  }

  return (
    <>
      <PageHeader
        title={t("Reconciliation")}
        description={t("Check the bank statement against what is recorded: each line becomes a settlement or a transfer that already exists, a new entry, or is ignored.")}
      />
      {canWrite && (
        <>
          <section className="card filters-card">
            <div className="filters">
              <label>
                {t("Statement account")}
                <select value={importAccount} onChange={(event) => setImportAccount(event.target.value)}>
                  <option value="">{t("Choose the account")}</option>
                  {activeAccounts.map((account) => (
                    <option key={account.id} value={account.id}>
                      {account.name}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </section>
          <CsvImport
            title={t("Import statement")}
            fileLabel={t("Bank OFX file")}
            accept=".ofx,.OFX,application/x-ofx"
            created={[t("line imported"), t("lines imported")]}
            blocked={importAccount === ""}
            run={(file, dryRun) => api.importStatement(companyId, importAccount, file, dryRun)}
            onImported={() => {
              setFilters({ account: importAccount, status: "pending" });
              void reload();
            }}
          >
            {t("The OFX statement that the bank offers for download. Importing the same file again, or one that overlaps, does not duplicate lines.")}
          </CsvImport>
        </>
      )}
      <section className="card list-card">
        <h2>{t("Statement lines")}</h2>
        <div className="filters" role="search">
          <label>
            {t("Account")}
            <select value={filters.account} onChange={(event) => setFilters({ ...filters, account: event.target.value })}>
              <option value="">{t("All accounts")}</option>
              {accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Status")}
            <select
              value={filters.status}
              onChange={(event) => setFilters({ ...filters, status: event.target.value as StatementLineFilters["status"] })}
            >
              <option value="pending">{t("Pending lines")}</option>
              <option value="matched">{t("Matched lines")}</option>
              <option value="ignored">{t("Ignored lines")}</option>
              <option value="">{t("All lines")}</option>
            </select>
          </label>
        </div>
        <Notice message={error ?? targetsError} />
        <Notice message={actionError} />
        <Notice message={notice} tone="success" />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        ) : items.length === 0 && error ? (
          <Button type="button" variant="ghost" onClick={() => void reload()}>
            {t("Try again")}
          </Button>
        ) : items.length === 0 ? (
          <div className="empty">
            <RotateCCWIcon size={40} aria-hidden />
            <p role="status">
              {filters.status === "pending" ? t("Nothing pending.") : t("No line found.")}
            </p>
            <p className="muted">{t("Import an account's statement to start.")}</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Date")}</th>
                  <th>{t("Description")}</th>
                  <th>{t("Account")}</th>
                  <th className="num">{t("Amount")}</th>
                  <th>{t("Status")}</th>
                  <th>
                    <span className="visually-hidden">{t("Actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {items.map((line, index) => (
                  <Fragment key={line.id}>
                    <ListRow index={index} highlighted={false}>
                      <td className="tabular">{formatDate(line.posted_on)}</td>
                      <td>{line.description}</td>
                      <td>{accountName(line.account_id)}</td>
                      <td className={line.amount.startsWith("-") ? "num negative" : "num"}>{formatMoney(line.amount)}</td>
                      <td>
                        <span className={STATUS_BADGES[line.status]}>{t(STATUS_LABELS[line.status])}</span>
                      </td>
                      <td className="row-actions">
                        {canWrite && line.status === "pending" && (
                          <>
                            <Button
                              type="button"
                              variant="ghost"
                              className="btn-small"
                              aria-label={t("Suggestions for {name}", { name: line.description })}
                              aria-expanded={open?.id === line.id && open.mode === "suggestions"}
                              disabled={busyId !== null}
                              onClick={() =>
                                setOpen(open?.id === line.id && open.mode === "suggestions" ? null : { id: line.id, mode: "suggestions" })
                              }
                            >
                              {t("Reconcile")}
                            </Button>
                            <Button
                              type="button"
                              variant="ghost"
                              className="btn-small"
                              aria-label={t("Create an entry for {name}", { name: line.description })}
                              aria-expanded={open?.id === line.id && open.mode === "entry"}
                              disabled={busyId !== null}
                              onClick={() =>
                                setOpen(open?.id === line.id && open.mode === "entry" ? null : { id: line.id, mode: "entry" })
                              }
                            >
                              {t("Record")}
                            </Button>
                            <Button
                              type="button"
                              variant="ghost"
                              className="btn-small"
                              aria-label={t("Ignore {name}", { name: line.description })}
                              pending={busyId === line.id}
                              disabled={busyId !== null}
                              onClick={() => void act(line, () => api.ignoreStatementLine(companyId, line.id), t("Line ignored."))}
                            >
                              {t("Ignore")}
                            </Button>
                          </>
                        )}
                        {canWrite && line.status === "matched" && (
                          <Button
                            type="button"
                            variant="ghost"
                            className="btn-small"
                            aria-label={t("Undo the reconciliation of {name}", { name: line.description })}
                            pending={busyId === line.id}
                            disabled={busyId !== null}
                            onClick={() => void act(line, () => api.unmatchStatementLine(companyId, line.id), t("Reconciliation undone."))}
                          >
                            {t("Undo")}
                          </Button>
                        )}
                        {canWrite && line.status === "ignored" && (
                          <Button
                            type="button"
                            variant="ghost"
                            className="btn-small"
                            aria-label={t("Restore {name}", { name: line.description })}
                            pending={busyId === line.id}
                            disabled={busyId !== null}
                            onClick={() => void act(line, () => api.restoreStatementLine(companyId, line.id), t("Line restored."))}
                          >
                            {t("Restore")}
                          </Button>
                        )}
                      </td>
                    </ListRow>
                    {open?.id === line.id && (
                      <tr>
                        <td colSpan={6} role="region" aria-label={open.mode === "suggestions" ? t("Suggestions for {name}", { name: line.description }) : t("Entry for {name}", { name: line.description })}>
                          {open.mode === "suggestions" ? (
                            <Suggestions
                              key={`${line.id}-${suggestionsVersion}`}
                              companyId={companyId}
                              line={line}
                              accountName={accountName}
                              busy={busyId !== null}
                              onMatch={(target) =>
                                void act(line, () => api.matchStatementLine(companyId, line.id, target), t("Line reconciled."))
                              }
                            />
                          ) : (
                            <EntryForm
                              line={line}
                              categories={categoryRows.map(({ category, depth }) => ({ id: category.id, label: `${"— ".repeat(depth)}${category.name}` }))}
                              busy={busyId !== null}
                              onCreate={(input) =>
                                void act(line, () => api.createEntryFromLine(companyId, line.id, input), t("Entry created and settled."))
                              }
                            />
                          )}
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
    </>
  );
}

/** What the books already have for a line: settlements and transfers of the same amount, within days. */
function Suggestions({
  companyId,
  line,
  accountName,
  busy,
  onMatch,
}: {
  companyId: string;
  line: StatementLine;
  accountName: (id: string) => string;
  busy: boolean;
  onMatch: (target: { settlement_id: string } | { transfer_id: string }) => void;
}) {
  const t = useT();
  const [suggestions, setSuggestions] = useState<LineSuggestion[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    api
      .statementLineSuggestions(companyId, line.id)
      .then((found) => current && setSuggestions(found))
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId, line.id]);

  if (error) {
    return <Notice message={error} />;
  }
  if (suggestions === null) {
    return (
      <p role="status" className="status">
        <Spinner /> {t("Searching…")}
      </p>
    );
  }
  if (suggestions.length === 0) {
    return (
      <p className="muted">
        {t("Nothing recorded with this amount, in this account, near this date. Use “Record” to create the entry from the line, or “Ignore”.")}
      </p>
    );
  }
  return (
    <ul className="history">
      {suggestions.map((suggestion) =>
        suggestion.kind === "settlement" ? (
          <li key={suggestion.settlement.id}>
            {t("Settlement of {description} on {date}, {amount} in the account {account}", {
              description: suggestion.entry_description,
              date: formatDate(suggestion.settlement.settled_on),
              amount: formatMoney(suggestion.settlement.cash),
              account: accountName(suggestion.settlement.bank_account_id),
            })}
            <Button
              type="button"
              variant="ghost"
              className="btn-small"
              icon={CheckIcon}
              aria-label={t("Reconcile with the settlement of {description}", { description: suggestion.entry_description })}
              disabled={busy}
              onClick={() => onMatch({ settlement_id: suggestion.settlement.id })}
            >
              {t("Reconcile")}
            </Button>
          </li>
        ) : (
          <li key={suggestion.transfer.id}>
            {t("Transfer of {amount} on {date}", { amount: formatMoney(suggestion.transfer.amount), date: formatDate(suggestion.transfer.occurred_on) })}
            {suggestion.transfer.note ? ` (${suggestion.transfer.note})` : ""}
            <Button
              type="button"
              variant="ghost"
              className="btn-small"
              icon={CheckIcon}
              aria-label={t("Reconcile with the transfer of {amount}", { amount: formatMoney(suggestion.transfer.amount) })}
              disabled={busy}
              onClick={() => onMatch({ transfer_id: suggestion.transfer.id })}
            >
              {t("Reconcile")}
            </Button>
          </li>
        ),
      )}
    </ul>
  );
}

/** Makes an entry of the line: one installment for its amount, settled on its day in its account. */
function EntryForm({
  line,
  categories,
  busy,
  onCreate,
}: {
  line: StatementLine;
  categories: { id: string; label: string }[];
  busy: boolean;
  onCreate: (input: { category_id: string; description?: string }) => void;
}) {
  const t = useT();
  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const description = String(form.get("description") ?? "").trim();
    onCreate({
      category_id: String(form.get("category_id") ?? ""),
      description: description === "" ? undefined : description,
    });
  }
  return (
    <form className="settle-form" onSubmit={submit}>
      <h3>{line.amount.startsWith("-") ? t("Record as a payable, already paid") : t("Record as a receivable, already received")}</h3>
      <label>
        {t("Category")}
        <select name="category_id" required defaultValue="">
          <option value="" disabled>
            {t("Choose a category")}
          </option>
          {categories.map((category) => (
            <option key={category.id} value={category.id}>
              {category.label}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("Description")}
        <input name="description" maxLength={200} autoComplete="off" defaultValue={line.description} />
      </label>
      <p className="muted">
        {t("Due and settled on {date}, for {amount}, in the statement account.", { date: formatDate(line.posted_on), amount: formatMoney(line.amount.replace("-", "")) })}
      </p>
      <Button type="submit" icon={CheckIcon} disabled={busy}>
        {t("Create entry")}
      </Button>
    </form>
  );
}
