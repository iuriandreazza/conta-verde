import { Fragment, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useParams } from "react-router-dom";
import {
  NO_BANK_ACCOUNT_FILTERS,
  api,
  errorMessage,
  type AccountBalance,
  type Balances,
  type BankAccount,
  type BankAccountFilters,
  type BankAccountInput,
} from "../api";
import { BankAccountHistory } from "../components/BankAccountHistory";
import { StatTile } from "../components/charts/StatTile";
import { ListFooter } from "../components/ListFooter";
import { Button } from "../components/Button";
import { ListRow } from "../components/ListRow";
import { OpeningBalanceForm } from "../components/OpeningBalanceForm";
import { BankAccountForm } from "../components/BankAccountForm";
import { Sheet } from "../components/Sheet";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { useLastValue } from "../lib/useLastValue";
import { PageHeader } from "../components/PageHeader";
import { Notice } from "../components/Notice";
import { Spinner } from "../components/Spinner";
import { BriefcaseBusinessIcon } from "../components/icons/briefcase-business";
import { HistoryIcon } from "../components/icons/history";
import { PlusIcon } from "../components/icons/plus";
import { BANK_ACCOUNT_KINDS } from "../lib/kinds";
import { usePagedList } from "../lib/usePagedList";
import { useCanWrite } from "../lib/useCanWrite";
import { accountDetails } from "../lib/banks";
import { formatMoney } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { CircleDollarSignIcon } from "../components/icons/circle-dollar-sign";
import { CreditCardIcon } from "../components/icons/credit-card";
import { HandCoinsIcon } from "../components/icons/hand-coins";
import { WalletIcon } from "../components/icons/wallet";
import type { AnimatedIcon } from "../components/Button";

const KIND_ICONS: Record<BankAccount["kind"], AnimatedIcon> = {
  checking: WalletIcon,
  savings: HandCoinsIcon,
  cash: CircleDollarSignIcon,
  card: CreditCardIcon,
};

function KindIcon({ kind, active }: { kind: BankAccount["kind"]; active: boolean }) {
  const Icon = KIND_ICONS[kind];
  return (
    <span className={active ? "kind-icon" : "kind-icon is-inactive"} aria-hidden>
      <Icon size={18} />
    </span>
  );
}
import { SquarePenIcon } from "../components/icons/square-pen";
import { StatusBadge } from "../components/StatusBadge";

/** What an account holds today. The opening balance it started from is in the form that sets it. */
function BalanceCell({ item }: { item: AccountBalance | undefined }) {
  if (item === undefined) {
    return <td className="num">—</td>;
  }
  return (
    <td
      className={item.balance.startsWith("-") ? "num negative" : "num"}
      title={
        item.opening === null
          ? "Sem saldo inicial: soma tudo o que foi baixado e transferido."
          : `Saldo inicial de ${formatMoney(item.opening.balance)} em ${formatDate(item.opening.on)}`
      }
    >
      {formatMoney(item.balance)}
    </td>
  );
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function BankAccountsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [filters, setFilters] = useState<BankAccountFilters>(NO_BANK_ACCOUNT_FILTERS);
  const [editing, setEditing] = useState<BankAccount | null>(null);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [highlightedId, setHighlightedId] = useState<string | null>(null);
  const [historyOpenFor, setHistoryOpenFor] = useState<string | null>(null);
  const [openingOpenFor, setOpeningOpenFor] = useState<string | null>(null);
  const [balances, setBalances] = useState<Balances | null>(null);
  const [balancesError, setBalancesError] = useState<string | null>(null);
  // Counts the reads: only the latest reply is used, so a slow earlier one cannot overwrite a newer.
  const latestBalances = useRef(0);

  const loadBalances = useCallback(async () => {
    const read = ++latestBalances.current;
    try {
      // Today where the person is, asked each time: the server does not know their zone (ADR 0031).
      const fresh = await api.getBalances(companyId, todayIso());
      if (read === latestBalances.current) {
        setBalances(fresh);
        setBalancesError(null);
      }
    } catch (failure) {
      if (read === latestBalances.current) {
        setBalancesError(errorMessage(failure));
      }
    }
  }, [companyId]);

  useEffect(() => {
    void loadBalances();
  }, [loadBalances]);

  const balanceOf = useMemo(
    () => new Map<string, AccountBalance>((balances?.accounts ?? []).map((item) => [item.account.id, item])),
    [balances],
  );

  const {
    items: accounts,
    setItems,
    nextCursor,
    loading,
    refreshing,
    loadingMore,
    error,
    applied,
    reload,
    loadMore,
  } = usePagedList<BankAccount, BankAccountFilters>(
    (applied, after) => api.listBankAccounts(companyId, applied, after),
    filters,
  );
  const isFiltered = applied.q !== "" || applied.kind !== "" || applied.active !== "";
  const { openCreate } = useCreation();
  const shown = useLastValue(editing);
  // An account made from the top bar or from here: read the list and the balances again, flash the row.
  useOnCreated("bankAccount", (created) => {
    setSaved(null);
    setHighlightedId(created.id);
    void Promise.all([reload(), loadBalances()]);
  });

  function closeSheet() {
    // A change on its way finishes first: closing now would hide what a failure has to say.
    if (saving) return;
    setEditing(null);
    setFormError(null);
  }

  async function handleSubmit(input: BankAccountInput) {
    if (editing === null) return;
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const updated = await api.updateBankAccount(companyId, editing.id, input);
      // In place: the list keeps its page and its filters.
      setItems((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setEditing(null);
      setSaved(`${updated.name} atualizada.`);
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <PageHeader title="Contas bancárias" description="Onde o dinheiro da empresa fica e circula: bancos, caixa e cartões." />
      <Notice message={balancesError} />
      {balances !== null && (
        <div className="kpi-row">
          <StatTile
            label={`Saldo consolidado em ${formatDate(balances.on)}`}
            value={formatMoney(balances.total)}
            note="Soma de todas as contas, inclusive as inativas."
          />
        </div>
      )}
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={PlusIcon} onClick={() => openCreate("bankAccount")}>
            Nova conta bancária
          </Button>
        </div>
      )}
      <Notice message={saved} tone="success" />
      <div>
      <section className="card list-card">
        <h2>Cadastradas</h2>
        <div className="filters" role="search">
          <label>
            Buscar
            <input
              type="search"
              value={filters.q}
              placeholder="Nome ou número da conta"
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
                setFilters({ ...filters, kind: event.target.value as BankAccountFilters["kind"] })
              }
            >
              <option value="">Todas</option>
              {BANK_ACCOUNT_KINDS.map((option) => (
                <option key={option.key} value={option.key}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Situação
            <select
              value={filters.active}
              onChange={(event) =>
                setFilters({ ...filters, active: event.target.value as BankAccountFilters["active"] })
              }
            >
              <option value="">Ativas e inativas</option>
              <option value="true">Só ativas</option>
              <option value="false">Só inativas</option>
            </select>
          </label>
          <label>
            Ordem
            <select
              value={filters.sort}
              onChange={(event) =>
                setFilters({ ...filters, sort: event.target.value as BankAccountFilters["sort"] })
              }
            >
              <option value="name">Nome (A–Z)</option>
              <option value="-name">Nome (Z–A)</option>
            </select>
          </label>
        </div>
        <Notice message={error} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        ) : accounts.length === 0 && error ? (
          <Button type="button" variant="ghost" onClick={() => void reload()}>
            Tentar de novo
          </Button>
        ) : accounts.length === 0 && isFiltered ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p role="status">Nenhum cadastro encontrado.</p>
            <Button
              type="button"
              variant="ghost"
              onClick={() => setFilters({ ...NO_BANK_ACCOUNT_FILTERS, sort: filters.sort })}
            >
              Limpar filtros
            </Button>
          </div>
        ) : accounts.length === 0 ? (
          <div className="empty">
            <BriefcaseBusinessIcon size={40} aria-hidden />
            <p>Nenhuma conta ainda.</p>
            <p className="muted">Uma conta corrente, a poupança, o caixa da loja ou um cartão.</p>
          </div>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Conta</th>
                  <th className="num">Saldo hoje</th>
                  <th>Situação</th>
                  <th>
                    <span className="visually-hidden">Ações</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {accounts.map((account, index) => (
                  <Fragment key={account.id}>
                    <ListRow index={index} highlighted={account.id === highlightedId}>
                      <td>
                        <div className="account-cell">
                          <KindIcon kind={account.kind} active={account.active} />
                          <div>
                            <div className="cell-title">{account.name}</div>
                            <div className="muted">{accountDetails(account)}</div>
                          </div>
                        </div>
                      </td>
                      <BalanceCell item={balanceOf.get(account.id)} />
                      <td><StatusBadge active={account.active} feminine /></td>
                      <td className="row-actions">
                        {canWrite && (
                          <Button
                            iconOnly
                            icon={CircleDollarSignIcon}
                            type="button"
                            variant="ghost"
                            aria-label={`Saldo inicial de ${account.name}`}
                            aria-expanded={openingOpenFor === account.id}
                            aria-controls={`opening-${account.id}`}
                            disabled={balances === null}
                            onClick={() =>
                              setOpeningOpenFor(openingOpenFor === account.id ? null : account.id)
                            }
                          >
                            Saldo inicial
                          </Button>
                        )}
                        {canWrite && (
                          <Button
                            iconOnly
                            icon={SquarePenIcon}
                            type="button"
                            variant="ghost"
                            aria-label={`Alterar ${account.name}`}
                            disabled={saving}
                            onClick={() => {
                              setEditing(account);
                              setFormError(null);
                              setSaved(null);
                            }}
                          >
                            Alterar
                          </Button>
                        )}
                        <Button
                          iconOnly
                          type="button"
                          variant="ghost"
                          aria-label={`Histórico de ${account.name}`}
                          aria-expanded={historyOpenFor === account.id}
                          aria-controls={`history-${account.id}`}
                          icon={HistoryIcon}
                          onClick={() =>
                            setHistoryOpenFor(historyOpenFor === account.id ? null : account.id)
                          }
                        >
                          Histórico
                        </Button>
                      </td>
                    </ListRow>
                    {openingOpenFor === account.id && balanceOf.has(account.id) && (
                      <tr id={`opening-${account.id}`}>
                        <td colSpan={4} role="region" aria-label={`Saldo inicial de ${account.name}`}>
                          <OpeningBalanceForm
                            companyId={companyId}
                            item={balanceOf.get(account.id) as AccountBalance}
                            onSaved={() => void loadBalances()}
                          />
                        </td>
                      </tr>
                    )}
                    {historyOpenFor === account.id && (
                      <tr id={`history-${account.id}`}>
                        <td colSpan={4} role="region" aria-label={`Histórico de ${account.name}`}>
                          <BankAccountHistory
                            // Remounted when the account changes, so an open history follows an edit.
                            key={JSON.stringify(account)}
                            companyId={companyId}
                            accountId={account.id}
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
        {!loading && accounts.length > 0 && (
          <ListFooter
            count={accounts.length}
            noun={["conta", "contas"]}
            hasMore={nextCursor !== null}
            loadingMore={loadingMore}
            refreshing={refreshing}
            onLoadMore={() => void loadMore()}
          />
        )}
      </section>
        {canWrite && (
          <Sheet open={editing !== null} title={`Alterar ${shown?.name ?? ""}`} onClose={closeSheet}>
            {shown !== null && (
              <BankAccountForm
                key={shown.id}
                account={shown}
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
