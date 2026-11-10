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

/** Links one bank account of the company to an account the aggregator holds, after asking which accounts a connection has. */
function ConnectForm({ companyId, accounts, onConnected }: { companyId: string; accounts: BankAccount[]; onConnected: (connection: BankConnection) => void }) {
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
      <h2>Conectar uma conta</h2>
      <form onSubmit={connect}>
        <label>
          Conta bancária no sistema
          <select value={bankAccountId} onChange={(event) => setBankAccountId(event.target.value)} required>
            <option value="">Escolha…</option>
            {accounts.map((account) => (
              <option key={account.id} value={account.id}>
                {account.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Código da conexão (Item ID)
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
            Buscar contas
          </Button>
        </div>
        {found !== null && found.length === 0 && <p className="muted">Essa conexão não tem contas.</p>}
        {found !== null && found.length > 0 && (
          <label>
            Conta no provedor
            <select value={accountId} onChange={(event) => setAccountId(event.target.value)} required>
              <option value="">Escolha…</option>
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
          Conectar
        </Button>
      </form>
    </section>
  );
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function BankConnectionsPage() {
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

  const nameOf = (id: string) => accounts.find((account) => account.id === id)?.name ?? "Conta removida";
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

  const sync = (connection: BankConnection) => act(connection, async () => setNotice(syncSummary(await api.syncBankConnection(companyId, connection.id))));
  const disconnect = (connection: BankConnection) => act(connection, async () => void (await api.disconnectBankConnection(companyId, connection.id)));

  return (
    <>
      <PageHeader
        title="Conexões bancárias"
        description="O extrato de uma conta bancária vem sozinho, todo dia, de um provedor de dados bancários (Open Finance), e entra na Conciliação como qualquer extrato importado. O sistema não guarda senha de banco nem credencial: só o vínculo. Sem provedor configurado no servidor, importe o arquivo OFX na Conciliação."
      />
      {!loading && !available && <Notice message="Nenhum provedor de dados bancários está configurado neste servidor (BANK_AGGREGATOR). Peça a quem cuida do servidor, ou importe o extrato em OFX na Conciliação." />}
      <Notice message={error} />
      <Notice tone="success" message={notice} />
      {canWrite && available && !loading && (
        <ConnectForm
          companyId={companyId}
          accounts={free}
          onConnected={() => {
            setNotice("Conta conectada. O extrato começa a vir na próxima sincronização, ou agora, em “Sincronizar”.");
            void load();
          }}
        />
      )}
      <section className="card list-card">
        <h2>Conexões</h2>
        {loading ? (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        ) : connections.length === 0 ? (
          <p className="muted">Nenhuma conta conectada ainda.</p>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Conta</th>
                  <th>Estado</th>
                  <th>Última sincronização</th>
                  <th>
                    <span className="visually-hidden">Ações</span>
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
                          {provider === connection.provider ? "" : `${connection.provider} · `}conectada por {connection.connected_by}
                        </span>
                      </td>
                      <td>
                        <span className={STATUS_BADGES[connection.status]}>{STATUS_LABELS[connection.status]}</span>
                        {connection.last_error && (
                          <>
                            <br />
                            <span className="muted">{connection.last_error}</span>
                          </>
                        )}
                      </td>
                      <td>{lastSync(connection)}</td>
                      <td>
                        <span className="form-actions">
                          {canWrite && connection.status !== "disconnected" && (
                            <>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={`Sincronizar ${nameOf(connection.bank_account_id)}`}
                                pending={busy === connection.id}
                                disabled={busy !== null || !available}
                                onClick={() => void sync(connection)}
                              >
                                Sincronizar
                              </Button>
                              {confirming === connection.id ? (
                                <Button
                                  type="button"
                                  variant="ghost"
                                  className="btn-small"
                                  aria-label={`Confirmar: desconectar ${nameOf(connection.bank_account_id)}`}
                                  disabled={busy !== null}
                                  onClick={() => void disconnect(connection)}
                                >
                                  Confirmar desconexão
                                </Button>
                              ) : (
                                <Button
                                  type="button"
                                  variant="ghost"
                                  className="btn-small"
                                  aria-label={`Desconectar ${nameOf(connection.bank_account_id)}`}
                                  disabled={busy !== null}
                                  onClick={() => setConfirming(connection.id)}
                                >
                                  Desconectar
                                </Button>
                              )}
                            </>
                          )}
                          <Button
                            type="button"
                            variant="ghost"
                            className="btn-small"
                            aria-label={`Histórico de ${nameOf(connection.bank_account_id)}`}
                            onClick={() => setHistoryOf(historyOf === connection.id ? null : connection.id)}
                          >
                            Histórico
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
