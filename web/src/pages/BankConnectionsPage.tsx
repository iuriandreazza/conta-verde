import { Fragment, useCallback, useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type AggregatedBankAccount, type BankAccount, type BankConnection } from "../api";
import { BankConnectionHistory } from "../components/BankConnectionHistory";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { STATUS_BADGES, STATUS_LABELS, lastSync, syncSummary } from "../lib/bankConnections";
import { readUpTo } from "../lib/readUpTo";
import { useCanWrite } from "../lib/useCanWrite";
import { useT } from "../i18n";

/** Links one bank account of the company to an account the aggregator holds, after asking which accounts a connection has. */
function ConnectForm({ companyId, accounts, onConnected }: { companyId: string; accounts: BankAccount[]; onConnected: (connection: BankConnection) => void }) {
  const t = useT();
  const [bankAccountId, setBankAccountId] = useState("");
  const [itemId, setItemId] = useState("");
  const [found, setFound] = useState<AggregatedBankAccount[] | null>(null);
  const [accountId, setAccountId] = useState("");
  const [looking, setLooking] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function look() {
    if (looking || itemId.trim() === "") return;
    setLooking(true);
    setError(null);
    setFound(null);
    setAccountId("");
    try {
      const reply = await api.aggregatedBankAccounts(companyId, itemId.trim());
      setFound(reply.items);
      if (reply.items.length === 1) setAccountId(reply.items[0].id);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setLooking(false);
    }
  }

  async function connect(event: FormEvent) {
    event.preventDefault();
    if (saving) return;
    setSaving(true);
    setError(null);
    try {
      onConnected(await api.connectBankAccount(companyId, { bank_account_id: bankAccountId, item_id: itemId.trim(), account_id: accountId }));
      setItemId("");
      setFound(null);
      setAccountId("");
      setBankAccountId("");
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="card">
      <h2>{t("Connect an account")}</h2>
      <form onSubmit={connect}>
        <label>
          {t("Bank account in the system")}
          <select value={bankAccountId} onChange={(event) => setBankAccountId(event.target.value)} required>
            <option value="">{t("Choose…")}</option>
            {accounts.map((account) => (
              <option key={account.id} value={account.id}>
                {account.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("Connection code (Item ID)")}
          <input
            value={itemId}
            onChange={(event) => {
              // The accounts found belong to the id that was looked up.
              setItemId(event.target.value);
              setFound(null);
              setAccountId("");
            }} autoComplete="off" required maxLength={200} />
        </label>
        <div className="form-actions">
          <Button type="button" variant="ghost" className="btn-small" pending={looking} disabled={itemId.trim() === ""} onClick={() => void look()}>
            {t("Find accounts")}
          </Button>
        </div>
        {found !== null && found.length === 0 && <p className="muted">{t("This connection has no accounts.")}</p>}
        {found !== null && found.length > 0 && (
          <label>
            {t("Account at the provider")}
            <select value={accountId} onChange={(event) => setAccountId(event.target.value)} required>
              <option value="">{t("Choose…")}</option>
              {found.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name}
                  {account.number ? ` · ${account.number}` : ""}
                </option>
              ))}
            </select>
          </label>
        )}
        <Notice message={error} />
        <Button type="submit" className="btn-small" pending={saving} disabled={bankAccountId === "" || accountId === ""}>
          {t("Connect")}
        </Button>
      </form>
    </section>
  );
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function BankConnectionsPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("finance");
  const [connections, setConnections] = useState<BankConnection[]>([]);
  const [accounts, setAccounts] = useState<BankAccount[]>([]);
  const [available, setAvailable] = useState(true);
  const [provider, setProvider] = useState("none");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  // Ending a connection stops the daily pull, so it asks once more.
  const [confirming, setConfirming] = useState<string | null>(null);
  const [historyOf, setHistoryOf] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [reply, bankAccounts] = await Promise.all([api.listBankConnections(companyId), readUpTo((after) => api.listBankAccounts(companyId, undefined, after))]);
      setConnections(reply.items);
      setAvailable(reply.available);
      setProvider(reply.provider);
      setAccounts(bankAccounts);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setLoading(false);
    }
  }, [companyId]);

  useEffect(() => {
    void load();
  }, [load]);

  const nameOf = (id: string) => accounts.find((account) => account.id === id)?.name ?? t("Removed account");
  // An account that already has a live connection is not offered again.
  const free = accounts.filter((account) => account.active && !connections.some((connection) => connection.bank_account_id === account.id && connection.status !== "disconnected"));

  async function act(connection: BankConnection, work: () => Promise<void>) {
    if (busy !== null) return;
    setBusy(connection.id);
    setError(null);
    setNotice(null);
    setConfirming(null);
    try {
      await work();
      await load();
    } catch (failure) {
      setError(errorMessage(failure));
      // A failed pull records why on the connection.
      await load();
    } finally {
      setBusy(null);
    }
  }

  const sync = (connection: BankConnection) => act(connection, async () => setNotice(syncSummary(await api.syncBankConnection(companyId, connection.id), t)));
  const disconnect = (connection: BankConnection) => act(connection, async () => void (await api.disconnectBankConnection(companyId, connection.id)));

  return (
    <>
      <PageHeader
        title={t("Bank connections")}
        description={t("The statement of a bank account comes on its own, every day, from a bank data provider (Open Finance), and enters Reconciliation like any imported statement. The system keeps no bank password or credential: only the link. With no provider set up on the server, import the OFX file in Reconciliation.")}
      />
      {!loading && !available && <Notice message={t("No bank data provider is set up on this server (BANK_AGGREGATOR). Ask whoever looks after the server, or import the OFX statement in Reconciliation.")} />}
      <Notice message={error} />
      <Notice tone="success" message={notice} />
      {canWrite && available && !loading && (
        <ConnectForm
          companyId={companyId}
          accounts={free}
          onConnected={() => {
            setNotice(t("Account connected. The statement starts to arrive at the next sync, or now, with “Sync”."));
            void load();
          }}
        />
      )}
      <section className="card list-card">
        <h2>{t("Connections")}</h2>
        {loading ? (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        ) : connections.length === 0 ? (
          <p className="muted">{t("No account connected yet.")}</p>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Account")}</th>
                  <th>{t("State")}</th>
                  <th>{t("Last sync")}</th>
                  <th>
                    <span className="visually-hidden">{t("Actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {connections.map((connection) => (
                  <Fragment key={connection.id}>
                    <tr>
                      <td>
                        {nameOf(connection.bank_account_id)}
                        <br />
                        <span className="muted">
                          {provider === connection.provider ? "" : `${connection.provider} · `}{t("connected by {name}", { name: connection.connected_by })}
                        </span>
                      </td>
                      <td>
                        <span className={STATUS_BADGES[connection.status]}>{t(STATUS_LABELS[connection.status])}</span>
                        {connection.last_error && (
                          <>
                            <br />
                            <span className="muted">{connection.last_error}</span>
                          </>
                        )}
                      </td>
                      <td>{lastSync(connection, t)}</td>
                      <td>
                        <span className="form-actions">
                          {canWrite && connection.status !== "disconnected" && (
                            <>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={t("Sync {name}", { name: nameOf(connection.bank_account_id) })}
                                pending={busy === connection.id}
                                disabled={busy !== null || !available}
                                onClick={() => void sync(connection)}
                              >
                                {t("Sync")}
                              </Button>
                              {confirming === connection.id ? (
                                <Button
                                  type="button"
                                  variant="ghost"
                                  className="btn-small"
                                  aria-label={t("Confirm: disconnect {name}", { name: nameOf(connection.bank_account_id) })}
                                  disabled={busy !== null}
                                  onClick={() => void disconnect(connection)}
                                >
                                  {t("Confirm disconnection")}
                                </Button>
                              ) : (
                                <Button
                                  type="button"
                                  variant="ghost"
                                  className="btn-small"
                                  aria-label={t("Disconnect {name}", { name: nameOf(connection.bank_account_id) })}
                                  disabled={busy !== null}
                                  onClick={() => setConfirming(connection.id)}
                                >
                                  {t("Disconnect")}
                                </Button>
                              )}
                            </>
                          )}
                          <Button
                            type="button"
                            variant="ghost"
                            className="btn-small"
                            aria-label={t("History of {name}", { name: nameOf(connection.bank_account_id) })}
                            onClick={() => setHistoryOf(historyOf === connection.id ? null : connection.id)}
                          >
                            {t("History")}
                          </Button>
                        </span>
                      </td>
                    </tr>
                    {historyOf === connection.id && (
                      <tr>
                        <td colSpan={4}>
                          <BankConnectionHistory key={connection.status} companyId={companyId} id={connection.id} accountName={nameOf} />
                        </td>
                      </tr>
                    )}
                  </Fragment>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </>
  );
}
