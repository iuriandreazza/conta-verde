import { Fragment, useCallback, useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type ApiKey, type IssuedApiKey } from "../api";
import { ApiKeyHistory } from "../components/ApiKeyHistory";
import { ApiKeySecret } from "../components/ApiKeySecret";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { SendIcon } from "../components/icons/send";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { GRACE_OPTIONS, STATUS_LABELS, scopeLabel } from "../lib/apiKeys";
import { useCanWrite } from "../lib/useCanWrite";

const BADGES: Record<ApiKey["status"], string> = { active: "badge-ok", expired: "badge-warning", revoked: "badge-muted" };

const formatDay = (day: string) => day.split("-").reverse().join("/");
const formatMoment = (millis: number) => new Date(millis).toLocaleDateString("pt-BR");

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function ApiKeysPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("members");
  const { openCreate } = useCreation();
  const [keys, setKeys] = useState<ApiKey[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // One key is changed at a time; rotating and revoking ask first.
  const [busyId, setBusyId] = useState<string | null>(null);
  const [rotatingId, setRotatingId] = useState<string | null>(null);
  const [grace, setGrace] = useState(2);
  const [revokingId, setRevokingId] = useState<string | null>(null);
  const [historyId, setHistoryId] = useState<string | null>(null);
  // The secret of a key made by rotating is shown here, once.
  const [issued, setIssued] = useState<IssuedApiKey | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      setKeys(await api.listApiKeys(companyId));
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setLoading(false);
    }
  }, [companyId]);

  useEffect(() => {
    void load();
  }, [load]);
  useOnCreated("apiKey", () => void load());

  async function change(key: ApiKey, action: () => Promise<void>) {
    if (busyId !== null) return;
    setBusyId(key.id);
    setError(null);
    try {
      await action();
      await load();
    } catch (failure) {
      await load();
      setError(errorMessage(failure));
    } finally {
      setBusyId(null);
      setRotatingId(null);
      setRevokingId(null);
    }
  }

  return (
    <>
      <PageHeader
        title="Chaves de API"
        description="Chaves para programas falarem com esta empresa pela API pública (lojas virtuais, automações). Cada chave tem seu escopo, aparece no histórico das alterações que fez e pode ser girada ou revogada."
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={SendIcon} onClick={() => openCreate("apiKey")}>
            Nova chave
          </Button>
        </div>
      )}
      {issued && <ApiKeySecret name={issued.key.name} secret={issued.secret} title="Nova chave" />}
      <section className="card list-card">
        <h2>Chaves</h2>
        <Notice message={error} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        ) : keys.length === 0 ? (
          <p className="muted">Nenhuma chave ainda.</p>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Nome</th>
                  <th>O que pode fazer</th>
                  <th>Estado</th>
                  <th>Uso</th>
                  <th>
                    <span className="visually-hidden">Ações</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {keys.map((key) => (
                  <Fragment key={key.id}>
                    <tr>
                      <td>
                        {key.name}
                        <br />
                        <span className="muted">
                          {key.label}… · criada por {key.created_by}
                        </span>
                      </td>
                      <td>
                        <ul className="plain-list">
                          {key.scopes.map((scope) => (
                            <li key={scope}>{scopeLabel(scope)}</li>
                          ))}
                        </ul>
                      </td>
                      <td>
                        <span className={`badge ${BADGES[key.status]}`}>{STATUS_LABELS[key.status]}</span>
                        {key.expires_at !== null && key.status === "active" && (
                          <>
                            <br />
                            <span className="muted">até {formatMoment(key.expires_at)}</span>
                          </>
                        )}
                      </td>
                      <td>{key.last_used_on ? `Usada em ${formatDay(key.last_used_on)}` : "Nunca usada"}</td>
                      <td>
                        <span className="form-actions">
                          <Button
                            type="button"
                            variant="ghost"
                            className="btn-small"
                            aria-label={`Histórico de ${key.name}`}
                            onClick={() => setHistoryId(historyId === key.id ? null : key.id)}
                          >
                            Histórico
                          </Button>
                          {canWrite && key.status === "active" && rotatingId !== key.id && revokingId !== key.id && (
                            <>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={`Girar ${key.name}`}
                                disabled={busyId !== null}
                                onClick={() => {
                                  setRevokingId(null);
                                  setRotatingId(key.id);
                                }}
                              >
                                Girar
                              </Button>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={`Revogar ${key.name}`}
                                disabled={busyId !== null}
                                onClick={() => {
                                  setRotatingId(null);
                                  setRevokingId(key.id);
                                }}
                              >
                                Revogar
                              </Button>
                            </>
                          )}
                        </span>
                        {rotatingId === key.id && (
                          <span className="form-actions">
                            <label>
                              A chave antiga continua valendo
                              <select value={grace} onChange={(event) => setGrace(Number(event.target.value))}>
                                {GRACE_OPTIONS.map((option, index) => (
                                  <option key={option.label} value={index}>
                                    {option.label}
                                  </option>
                                ))}
                              </select>
                            </label>
                            <Button
                              type="button"
                              className="btn-small"
                              aria-label={`Confirmar a rotação de ${key.name}`}
                              pending={busyId === key.id}
                              disabled={busyId !== null}
                              onClick={() =>
                                void change(key, async () => {
                                  setIssued(await api.rotateApiKey(companyId, key.id, GRACE_OPTIONS[grace].hours));
                                })
                              }
                            >
                              Girar
                            </Button>
                            <Button type="button" variant="ghost" className="btn-small" onClick={() => setRotatingId(null)}>
                              Cancelar
                            </Button>
                          </span>
                        )}
                        {revokingId === key.id && (
                          <span className="form-actions">
                            <Button
                              type="button"
                              className="btn-small"
                              aria-label={`Confirmar a revogação de ${key.name}`}
                              pending={busyId === key.id}
                              disabled={busyId !== null}
                              onClick={() =>
                                void change(key, async () => {
                                  await api.revokeApiKey(companyId, key.id);
                                })
                              }
                            >
                              Revogar agora
                            </Button>
                            <Button type="button" variant="ghost" className="btn-small" onClick={() => setRevokingId(null)}>
                              Cancelar
                            </Button>
                          </span>
                        )}
                      </td>
                    </tr>
                    {historyId === key.id && (
                      <tr>
                        <td colSpan={5}>
                          <ApiKeyHistory companyId={companyId} keyId={key.id} />
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
