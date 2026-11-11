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
import { useT } from "../i18n";
import { useCanWrite } from "../lib/useCanWrite";

/** Makes a connector of a kind of the catalogue, off, for a store the person names. */
function CreateForm({ companyId, list, onCreated }: { companyId: string; list: ConnectorList; onCreated: () => void }) {
  const t = useT();
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
      <h2>{t("New connector")}</h2>
      <form onSubmit={create}>
        <label>
          {t("Type")}
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
            {t(described.what)} <strong>{t("Needs:")}</strong> {t(described.needs)}
          </p>
        )}
        <label>
          {t("Store name")}
          <input value={name} onChange={(event) => setName(event.target.value)} required maxLength={100} autoComplete="off" placeholder={t("Downtown store")} />
        </label>
        <Notice message={error} />
        <Button type="submit" className="btn-small" pending={saving} disabled={name.trim() === "" || kind === ""}>
          {t("Create")}
        </Button>
      </form>
    </section>
  );
}

/** Asks where the package listens, and turns the connector on. */
function TurnOnForm({ companyId, connector, onTurnedOn }: { companyId: string; connector: Connector; onTurnedOn: (result: TurnedOnConnector) => void }) {
  const t = useT();
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
        {t("Address of the package to receive the notices (https)")}
        <input
          type="url"
          value={address}
          onChange={(event) => setAddress(event.target.value)}
          required
          maxLength={2000}
          autoComplete="off"
          placeholder={t("https://connector.my-store.example.com/webhooks/conta-verde")}
        />
      </label>
      <p className="muted">{t("Turning on makes an API key (only what the package needs) and a webhook for this address. Nothing from the store is kept here.")}</p>
      <Notice message={error} />
      <Button type="submit" className="btn-small" pending={saving} disabled={address.trim() === ""}>
        {t("Turn on")}
      </Button>
    </form>
  );
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function ConnectorsPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("members");
  const [list, setList] = useState<ConnectorList | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [turning, setTurning] = useState<string | null>(null);
  const [historyOf, setHistoryOf] = useState<string | null>(null);
  // The secrets of a connector just turned on are shown here, once.
  const [shown, setShown] = useState<TurnedOnConnector[]>([]);

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
      setNotice(t("“{name}” turned off: the key was revoked and the webhook was turned off.", { name: connector.name }));
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
        title={t("Connectors")}
        description={t("A connector links your online store to the system. It is a separate package, which runs wherever you put it and talks only through the public API and the webhooks: turning it on here only makes the API key and the webhook it uses, and turning it off revokes the key and turns the webhook off. The store's key and password stay only in the package.")}
      />
      <Notice message={error} />
      <Notice tone="success" message={notice} />
      {list && !list.available && (
        <Notice message={t("Connectors are off: the server has no key to keep the webhooks' secrets (FISCAL_SECRETS_KEY). Ask whoever looks after the server.")} />
      )}
      {shown.map((turned) => (
        <ConnectorSecrets
          key={turned.connector.id}
          name={turned.connector.name}
          apiKey={turned.api_key_secret}
          webhookSecret={turned.webhook_secret}
          packageLink={packageUrl(SOURCE_URL, packagePath(turned.connector.kind))}
        />
      ))}
      {shown.length > 0 && (
        <Button type="button" variant="ghost" className="btn-small" onClick={() => setShown([])}>
          {t("I already copied")}
        </Button>
      )}
      {canWrite && list && list.available && (
        <CreateForm
          companyId={companyId}
          list={list}
          onCreated={() => {
            setNotice(t("Connector created. Turn it on when the package is up."));
            void load();
          }}
        />
      )}
      <section className="card list-card">
        <h2>{t("Company connectors")}</h2>
        {list === null ? (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        ) : list.items.length === 0 ? (
          <p className="muted">{t("No connectors yet.")}</p>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Store")}</th>
                  <th>{t("Status")}</th>
                  <th>
                    <span className="visually-hidden">{t("Actions")}</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {list.items.map((connector) => {
                  const reason = attentionReason(connector, t);
                  return (
                    <Fragment key={connector.id}>
                      <tr>
                        <td>
                          {connector.name}
                          <br />
                          <span className="muted">
                            {t("{kind} · created by {name}", { kind: kindTitle(connector.kind), name: connector.created_by })}
                          </span>
                        </td>
                        <td>
                          <span className={STATE_BADGES[connector.state]}>{t(STATE_LABELS[connector.state])}</span>
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
                              <Button type="button" variant="ghost" className="btn-small" aria-label={t("Turn on {name}", { name: connector.name })} disabled={busy !== null || !list.available} onClick={() => setTurning(turning === connector.id ? null : connector.id)}>
                                {t("Turn on")}
                              </Button>
                            )}
                            {canWrite && connector.active && (
                              <Button type="button" variant="ghost" className="btn-small" aria-label={t("Turn off {name}", { name: connector.name })} pending={busy === connector.id} disabled={busy !== null} onClick={() => void turnOff(connector)}>
                                {t("Turn off")}
                              </Button>
                            )}
                            <Button type="button" variant="ghost" className="btn-small" aria-label={t("History of {name}", { name: connector.name })} onClick={() => setHistoryOf(historyOf === connector.id ? null : connector.id)}>
                              {t("History")}
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
                                setShown((held) => [...held, result]);
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
