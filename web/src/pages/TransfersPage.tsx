import { useEffect, useMemo, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import {
  NO_TRANSFER_FILTERS,
  api,
  errorMessage,
  type BankAccount,
  type Transfer,
  type TransferFilters,
  type TransferInput,
} from "../api";
import { Button } from "../components/Button";
import { ListRow } from "../components/ListRow";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { ChevronDownIcon } from "../components/icons/chevron-down";
import { RotateCCWIcon } from "../components/icons/rotate-ccw";
import { SendIcon } from "../components/icons/send";
import { formatMoney, isAmbiguousDecimal } from "../lib/decimal";
import { centsToApi, formatDate, isIsoDate, toCents } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { todayIso } from "../lib/settlement";
import { useCanWrite } from "../lib/useCanWrite";
import { usePagedList } from "../lib/usePagedList";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance: the old list is dropped and a reply still in flight for another company lands on an
// unmounted component.
export function TransfersPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [filters, setFilters] = useState<TransferFilters>(NO_TRANSFER_FILTERS);
  const [accounts, setAccounts] = useState<BankAccount[]>([]);
  const [accountsError, setAccountsError] = useState<string | null>(null);
  // Bumped after a transfer so the form starts over.
  const [formVersion, setFormVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
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
  const activeAccounts = accounts.filter((account) => account.active);

  const { items, nextCursor, loading, loadingMore, error, applied, reload, loadMore } = usePagedList<
    Transfer,
    TransferFilters
  >((applied, after) => api.listTransfers(companyId, applied, after), filters);
  const isFiltered = applied.account !== "";

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    const typedAmount = field("amount");
    const cents = isAmbiguousDecimal(typedAmount) ? null : toCents(typedAmount);
    if (field("from_account_id") === field("to_account_id")) {
      setFormError("Escolha duas contas diferentes.");
      return;
    }
    if (cents === null || cents === 0n) {
      setFormError("Informe o valor em reais e centavos, por exemplo 1.234,50.");
      return;
    }
    if (!isIsoDate(field("occurred_on"))) {
      setFormError("Informe a data da transferência.");
      return;
    }
    const input: TransferInput = {
      from_account_id: field("from_account_id"),
      to_account_id: field("to_account_id"),
      amount: centsToApi(cents),
      occurred_on: field("occurred_on"),
      note: field("note") || undefined,
    };
    setSaving(true);
    setFormError(null);
    setSaved(null);
    try {
      const created = await api.createTransfer(companyId, input);
      setHighlightedId(created.id);
      setSaved(`Transferência de ${formatMoney(created.amount)} registrada.`);
      setFormVersion((version) => version + 1);
      await reload();
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

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
        title="Transferências"
        description="Dinheiro que muda de uma conta da empresa para outra, como um depósito ou o pagamento da fatura do cartão. Não entra no resultado."
      />
      <div className={canWrite ? "split" : "split split-single"}>
        <section className="card list-card">
          <h2>Registradas</h2>
          <div className="filters" role="search">
            <label>
              Conta
              <select
                value={filters.account}
                onChange={(event) => setFilters({ account: event.target.value })}
              >
                <option value="">Todas as contas</option>
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
              <Spinner /> Carregando…
            </p>
          ) : items.length === 0 && error ? (
            <Button type="button" variant="ghost" onClick={() => void reload()}>
              Tentar de novo
            </Button>
          ) : items.length === 0 ? (
            <div className="empty">
              <SendIcon size={40} aria-hidden />
              <p role="status">{isFiltered ? "Nenhuma transferência nessa conta." : "Nenhuma transferência ainda."}</p>
              {isFiltered ? (
                <Button type="button" variant="ghost" onClick={() => setFilters(NO_TRANSFER_FILTERS)}>
                  Limpar filtro
                </Button>
              ) : (
                <p className="muted">Use o formulário para mover dinheiro entre duas contas.</p>
              )}
            </div>
          ) : (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Data</th>
                    <th>De</th>
                    <th>Para</th>
                    <th className="num">Valor</th>
                    <th>Observação</th>
                    <th>Situação</th>
                    <th>
                      <span className="visually-hidden">Ações</span>
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
                          <span className="badge badge-muted">Estornada</span>
                        ) : (
                          <span className="badge badge-dot badge-ok">Válida</span>
                        )}
                      </td>
                      <td className="row-actions">
                        {canWrite && !transfer.reversed && (
                          <Button
                            iconOnly
                            icon={RotateCCWIcon}
                            type="button"
                            variant="ghost"
                            aria-label={`Estornar a transferência de ${formatMoney(transfer.amount)} em ${formatDate(transfer.occurred_on)}`}
                            pending={reversingId === transfer.id}
                            disabled={reversingId !== null}
                            onClick={() => void reverse(transfer)}
                          >
                            Estornar
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
              Carregar mais
            </Button>
          )}
        </section>
        {canWrite && (
          <aside className="card form-card">
            <h2>Nova transferência</h2>
            <form key={formVersion} onSubmit={(event) => void handleSubmit(event)}>
              <label>
                De
                <select name="from_account_id" required defaultValue="">
                  <option value="" disabled>
                    Escolha a conta de origem
                  </option>
                  {activeAccounts.map((account) => (
                    <option key={account.id} value={account.id}>
                      {account.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Para
                <select name="to_account_id" required defaultValue="">
                  <option value="" disabled>
                    Escolha a conta de destino
                  </option>
                  {activeAccounts.map((account) => (
                    <option key={account.id} value={account.id}>
                      {account.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Valor
                <input name="amount" inputMode="decimal" autoComplete="off" required placeholder="0,00" />
              </label>
              <label>
                Data
                <input name="occurred_on" type="date" required defaultValue={todayIso()} />
              </label>
              <label>
                Observação
                <input name="note" maxLength={200} autoComplete="off" placeholder="Opcional" />
              </label>
              <Notice message={formError} />
              <div className="form-actions">
                <Button type="submit" icon={SendIcon} pending={saving}>
                  Transferir
                </Button>
              </div>
            </form>
            <Notice message={saved} tone="success" />
          </aside>
        )}
      </div>
    </>
  );
}
