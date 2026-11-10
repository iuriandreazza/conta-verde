import { Fragment, useCallback, useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type Connector, type ConnectorList, type TurnedOnConnector } from "../api";
import { Button } from "../components/Button";
import { ConnectorHistory } from "../components/ConnectorHistory";
import { ConnectorSecrets } from "../components/ConnectorSecrets";
import { SOURCE_URL } from "../components/Footer";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { KIND_DESCRIPTIONS, STATE_BADGES, STATE_LABELS, attentionReason, packageUrl } from "../lib/connectors";
import { useCanWrite } from "../lib/useCanWrite";

/** Makes a connector of a kind of the catalogue, off, for a store the person names. */
function CreateForm({ companyId, list, onCreated }: { companyId: string; list: ConnectorList; onCreated: () => void }) {
  const [kind, setKind] = useState(list.catalogue[0]?.kind ?? "");
  const [name, setName] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function create(event: FormEvent) {
    event.preventDefault();
    if (saving) return;
    setSaving(true);
    setError(null);
    try {
      await api.createConnector(companyId, { kind, name: name.trim() });
      setName("");
      onCreated();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  const described = KIND_DESCRIPTIONS[kind];
  return (
    <section className="card">
      <h2>Novo conector</h2>
      <form onSubmit={create}>
        <label>
          Tipo
          <select value={kind} onChange={(event) => setKind(event.target.value)}>
            {list.catalogue.map((entry) => (
              <option key={entry.kind} value={entry.kind}>
                {entry.title}
              </option>
            ))}
          </select>
        </label>
        {described && (
          <p className="muted">
            {described.what} <strong>Precisa de:</strong> {described.needs}
          </p>
        )}
        <label>
          Nome da loja
          <input value={name} onChange={(event) => setName(event.target.value)} required maxLength={100} autoComplete="off" placeholder="Loja do centro" />
        </label>
        <Notice message={error} />
        <Button type="submit" className="btn-small" pending={saving} disabled={name.trim() === "" || kind === ""}>
          Criar
        </Button>
      </form>
    </section>
  );
}

/** Asks where the package listens, and turns the connector on. */
function TurnOnForm({ companyId, connector, onTurnedOn }: { companyId: string; connector: Connector; onTurnedOn: (result: TurnedOnConnector) => void }) {
  const [address, setAddress] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function turnOn(event: FormEvent) {
    event.preventDefault();
    if (saving) return;
    setSaving(true);
    setError(null);
    try {
      onTurnedOn(await api.activateConnector(companyId, connector.id, address.trim()));
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  return (
    <form onSubmit={turnOn}>
      <label>
        Endereço do pacote para receber os avisos (https)
        <input
          type="url"
          value={address}
          onChange={(event) => setAddress(event.target.value)}
          required
          maxLength={2000}
          autoComplete="off"
          placeholder="https://conector.minha-loja.example.com/webhooks/conta-verde"
        />
      </label>
      <p className="muted">Ligar faz uma chave de API (só o que o pacote precisa) e um webhook para esse endereço. Nada da loja fica guardado aqui.</p>
      <Notice message={error} />
      <Button type="submit" className="btn-small" pending={saving} disabled={address.trim() === ""}>
        Ligar
      </Button>
    </form>
  );
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function ConnectorsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("members");
  const [list, setList] = useState<ConnectorList | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [turning, setTurning] = useState<string | null>(null);
  const [historyOf, setHistoryOf] = useState<string | null>(null);
  // The secrets of a connector just turned on are shown here, once.
  const [shown, setShown] = useState<TurnedOnConnector | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setList(await api.listConnectors(companyId));
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }, [companyId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function turnOff(connector: Connector) {
    if (busy !== null) return;
    setBusy(connector.id);
    setError(null);
    setNotice(null);
    try {
      await api.deactivateConnector(companyId, connector.id);
      setNotice(`“${connector.name}” desligado: a chave foi revogada e o webhook, desligado.`);
      await load();
    } catch (failure) {
      setError(errorMessage(failure));
      await load();
    } finally {
      setBusy(null);
    }
  }

  const kindTitle = (kind: string) => list?.catalogue.find((entry) => entry.kind === kind)?.title ?? kind;
  const packagePath = (kind: string) => list?.catalogue.find((entry) => entry.kind === kind)?.package_path ?? "connectors";

  return (
    <>
      <PageHeader
        title="Conectores"
        description="Um conector liga a sua loja virtual ao sistema. Ele é um pacote à parte, que roda onde você colocar e conversa só pela API pública e pelos webhooks: ligar aqui só cria a chave de API e o webhook que ele usa, e desligar revoga a chave e desliga o webhook. A chave e a senha da loja ficam só no pacote."
      />
      <Notice message={error} />
      <Notice tone="success" message={notice} />
      {list && !list.available && (
        <Notice message="Conectores desligados: o servidor não tem a chave para guardar os segredos dos webhooks (FISCAL_SECRETS_KEY). Peça a quem cuida do servidor." />
      )}
      {shown && (
        <>
          <ConnectorSecrets
            name={shown.connector.name}
            apiKey={shown.api_key_secret}
            webhookSecret={shown.webhook_secret}
            packageLink={packageUrl(SOURCE_URL, packagePath(shown.connector.kind))}
          />
          <Button type="button" variant="ghost" className="btn-small" onClick={() => setShown(null)}>
            Já copiei
          </Button>
        </>
      )}
      {canWrite && list && list.available && (
        <CreateForm
          companyId={companyId}
          list={list}
          onCreated={() => {
            setNotice("Conector criado. Ligue-o quando o pacote estiver no ar.");
            void load();
          }}
        />
      )}
      <section className="card list-card">
        <h2>Conectores da empresa</h2>
        {list === null ? (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        ) : list.items.length === 0 ? (
          <p className="muted">Nenhum conector ainda.</p>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Loja</th>
                  <th>Estado</th>
                  <th>
                    <span className="visually-hidden">Ações</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {list.items.map((connector) => {
                  const reason = attentionReason(connector);
                  return (
                    <Fragment key={connector.id}>
                      <tr>
                        <td>
                          {connector.name}
                          <br />
                          <span className="muted">
                            {kindTitle(connector.kind)} · criado por {connector.created_by}
                          </span>
                        </td>
                        <td>
                          <span className={STATE_BADGES[connector.state]}>{STATE_LABELS[connector.state]}</span>
                          {reason && (
                            <>
                              <br />
                              <span className="muted">{reason}</span>
                            </>
                          )}
                        </td>
                        <td>
                          <span className="form-actions">
                            {canWrite && !connector.active && (
                              <Button type="button" variant="ghost" className="btn-small" aria-label={`Ligar ${connector.name}`} disabled={busy !== null || !list.available} onClick={() => setTurning(turning === connector.id ? null : connector.id)}>
                                Ligar
                              </Button>
                            )}
                            {canWrite && connector.active && (
                              <Button type="button" variant="ghost" className="btn-small" aria-label={`Desligar ${connector.name}`} pending={busy === connector.id} disabled={busy !== null} onClick={() => void turnOff(connector)}>
                                Desligar
                              </Button>
                            )}
                            <Button type="button" variant="ghost" className="btn-small" aria-label={`Histórico de ${connector.name}`} onClick={() => setHistoryOf(historyOf === connector.id ? null : connector.id)}>
                              Histórico
                            </Button>
                          </span>
                        </td>
                      </tr>
                      {turning === connector.id && !connector.active && (
                        <tr>
                          <td colSpan={3}>
                            <TurnOnForm
                              companyId={companyId}
                              connector={connector}
                              onTurnedOn={(result) => {
                                setTurning(null);
                                setNotice(null);
                                setShown(result);
                                void load();
                              }}
                            />
                          </td>
                        </tr>
                      )}
                      {historyOf === connector.id && (
                        <tr>
                          <td colSpan={3}>
                            <ConnectorHistory key={`${connector.state}`} companyId={companyId} id={connector.id} />
                          </td>
                        </tr>
                      )}
                    </Fragment>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </>
  );
}
