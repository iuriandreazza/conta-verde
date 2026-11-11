import { useEffect, useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_TRANSFER_FILTERS,
  api,
  errorMessage,
  type BankAccount,
  type Transfer,
  type TransferFilters,
} from "../api";
import { Button } from "../components/Button";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { RotateCCWIcon } from "../components/icons/rotate-ccw";
import { SendIcon } from "../components/icons/send";
import { PlusIcon } from "../components/icons/plus";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { formatMoney } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";
import { useT } from "../i18n";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function TransfersPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [filters, setFilters] = useState<TransferFilters>(NO_TRANSFER_FILTERS);
  const [accounts, setAccounts] = useState<BankAccount[]>([]);
  const [accountsError, setAccountsError] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [reversingId, setReversingId] = useState<string | null>(null);
  const [listNotice, setListNotice] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    readUpTo((after) => api.listBankAccounts(companyId, undefined, after))
      .then((list) => current && setAccounts(list))
      .catch((failure: unknown) => current && setAccountsError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  const names = useMemo(() => new Map(accounts.map((account) => [account.id, account.name])), [accounts]);

  const { items, nextCursor, loading, loadingMore, error, applied, reload, loadMore } = usePagedList<
    Transfer,
    TransferFilters
  >((applied, after) => api.listTransfers(companyId, applied, after), filters);
  const isFiltered = applied.account !== "";

  const { openCreate } = useCreation();
  // A transfer made from the top bar or from here: read the list again and flash the new row.
  useOnCreated("transfer", (created) => {
    setHighlightedId(created.id);
    void reload();
  });

  async function reverse(transfer: Transfer) {
    if (reversingId !== null) {
      return;
    }
    setReversingId(transfer.id);
    setListNotice(null);
    try {
      await api.reverseTransfer(companyId, transfer.id);
      await reload();
    } catch (failure) {
      // Another tab may have reversed it already: read what is true now, then say what happened.
      await reload();
      setListNotice(errorMessage(failure));
    } finally {
      setReversingId(null);
    }
  }

  return (
    <>
      <PageHeader
        title={t("Transfers")}
        description={t("Money that moves from one company account to another, like a deposit or paying the card bill. It does not enter the result.")}
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("transfer")}>
            {t("New transfer")}
          </Button>
        </div>
      )}
      <div>
        <section className="card list-card">
          <h2>{t("Registered transfers")}</h2>
          <div className="filters" role="search">
            <label>
              {t("Account")}
              <select
                value={filters.account}
                onChange={(event) => setFilters({ account: event.target.value })}
              >
                <option value="">{t("All accounts")}</option>
                {accounts.map((account) => (
                  <option key={account.id} value={account.id}>
                    {account.name}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <Notice message={accountsError} />
          <Notice message={error} />
          <Notice message={listNotice} />
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
              <SendIcon size={40} aria-hidden />
              <p role="status">{isFiltered ? t("No transfer in this account.") : t("No transfers yet.")}</p>
              {isFiltered ? (
                <Button type="button" variant="ghost" onClick={() => setFilters(NO_TRANSFER_FILTERS)}>
                  {t("Clear filter")}
                </Button>
              ) : (
                <p className="muted">{t("Use the form to move money between two accounts.")}</p>
              )}
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>{t("Date")}</th>
                    <th>{t("From")}</th>
                    <th>{t("To account")}</th>
                    <th className="num">{t("Amount")}</th>
                    <th>{t("Note")}</th>
                    <th>{t("Status")}</th>
                    <th>
                      <span className="visually-hidden">{t("Actions")}</span>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((transfer, index) => (
                    <ListRow
                      key={transfer.id}
                      index={index}
                      highlighted={transfer.id === highlightedId}
                    >
                      <td className="tabular">{formatDate(transfer.occurred_on)}</td>
                      <td>{names.get(transfer.from_account_id) ?? "—"}</td>
                      <td>{names.get(transfer.to_account_id) ?? "—"}</td>
                      <td className={transfer.reversed ? "num muted" : "num"}>
                        {formatMoney(transfer.amount)}
                      </td>
                      <td>{transfer.note ?? "—"}</td>
                      <td>
                        {transfer.reversed ? (
                          <span className="badge badge-muted">{t("Reversed")}</span>
                        ) : (
                          <span className="badge badge-dot badge-ok">{t("Valid")}</span>
                        )}
                      </td>
                      <td className="row-actions">
                        {canWrite && !transfer.reversed && (
                          <Button
                            iconOnly
                            icon={RotateCCWIcon}
                            type="button"
                            variant="ghost"
                            aria-label={t("Reverse the transfer of {amount} on {date}", { amount: formatMoney(transfer.amount), date: formatDate(transfer.occurred_on) })}
                            pending={reversingId === transfer.id}
                            disabled={reversingId !== null}
                            onClick={() => void reverse(transfer)}
                          >
                            {t("Reverse")}
                          </Button>
                        )}
                      </td>
                    </ListRow>
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
      </div>
    </>
  );
}
