import { Fragment, useCallback, useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type WebhookDelivery, type WebhookSubscription } from "../api";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { WebhookHistory } from "../components/WebhookHistory";
import { WebhookSecret } from "../components/WebhookSecret";
import { SendIcon } from "../components/icons/send";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { DELIVERY_LABELS, EVENT_LABELS, eventLabel, eventsText } from "../lib/webhooks";
import { useCanWrite } from "../lib/useCanWrite";

type Panel = "log" | "secret" | "edit" | "history";

const BADGES: Record<WebhookDelivery["status"], string> = { pending: "badge-warning", delivered: "badge-ok", failed: "badge-critical" };

const formatMoment = (millis: number) => new Date(millis).toLocaleString("pt-BR");

/** The log of what was sent to one subscription, newest first, with the way to send one again. */
function DeliveryLog({ companyId, subscription, canWrite, refresh }: { companyId: string; subscription: WebhookSubscription; canWrite: boolean; refresh: number }) {
  const [items, setItems] = useState<WebhookDelivery[] | null>(null);
  const [next, setNext] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let current = true;
    setError(null);
    api
      .webhookDeliveries(companyId, subscription.id)
      .then((page) => {
        if (!current) return;
        setItems(page.items);
        setNext(page.next_cursor);
      })
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId, subscription.id, version, refresh]);

  async function more() {
    if (next === null) return;
    try {
      const page = await api.webhookDeliveries(companyId, subscription.id, next);
      setItems((held) => [...(held ?? []), ...page.items]);
      setNext(page.next_cursor);
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  async function again(delivery: WebhookDelivery) {
    if (busy !== null) return;
    setBusy(delivery.id);
    setError(null);
    try {
      await api.redeliverWebhook(companyId, subscription.id, delivery.id);
      setVersion((held) => held + 1);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setBusy(null);
    }
  }

  if (items === null && error === null) {
    return (
      <p role="status" className="status">
        <Spinner size={16} /> Carregando entregas…
      </p>
    );
  }
  return (
    <>
      <Notice message={error} />
      {items !== null && items.length === 0 && <p className="muted">Nenhuma entrega ainda.</p>}
      {items !== null && items.length > 0 && (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Evento</th>
                <th>Estado</th>
                <th>Tentativas</th>
                <th>Última resposta</th>
                <th>Quando</th>
                <th>
                  <span className="visually-hidden">Ações</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {items.map((delivery) => (
                <tr key={delivery.id}>
                  <td>{eventLabel(delivery.kind)}</td>
                  <td>
                    <span className={`badge ${BADGES[delivery.status]}`}>{DELIVERY_LABELS[delivery.status]}</span>
                  </td>
                  <td>{delivery.attempts}</td>
                  <td>{delivery.last_error ?? (delivery.last_status ? `Resposta ${delivery.last_status}` : "—")}</td>
                  <td>{formatMoment(delivery.created_at)}</td>
                  <td>
                    {canWrite && (
                      <Button
                        type="button"
                        variant="ghost"
                        className="btn-small"
                        aria-label={`Enviar de novo: ${eventLabel(delivery.kind)} de ${formatMoment(delivery.created_at)}`}
                        pending={busy === delivery.id}
                        disabled={busy !== null}
                        onClick={() => void again(delivery)}
                      >
                        Enviar de novo
                      </Button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {next !== null && (
        <Button type="button" variant="ghost" className="btn-small" onClick={() => void more()}>
          Ver mais
        </Button>
      )}
      <p className="muted">Guardamos as entregas por 30 dias. Só o código da resposta e um motivo curto ficam registrados, nunca o que o destino respondeu.</p>
    </>
  );
}

/** Replaces the secret: with one of the person's own, or with one made here and shown once. */
function SecretPanel({ companyId, subscription, onMade }: { companyId: string; subscription: WebhookSubscription; onMade: (secret: string) => void }) {
  const [typed, setTyped] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  async function replace(secret?: string) {
    if (saving) return;
    setSaving(true);
    setError(null);
    setDone(null);
    try {
      const reply = await api.replaceWebhookSecret(companyId, subscription.id, secret);
      if (reply.secret !== null) {
        onMade(reply.secret);
      } else {
        setDone("Segredo trocado. O anterior continua assinando por 24 horas.");
        setTyped("");
      }
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      <p className="muted">
        O segredo é guardado cifrado e nunca é mostrado de novo. Ao trocar, o anterior continua assinando por 24 horas, para quem recebe trocar sem corte.
      </p>
      <Notice message={error} />
      <Notice tone="success" message={done} />
      <div className="form-actions">
        <Button type="button" className="btn-small" pending={saving} onClick={() => void replace()}>
          Gerar um novo segredo
        </Button>
      </div>
      <form
        className="form-actions"
        onSubmit={(event: FormEvent) => {
          event.preventDefault();
          void replace(typed);
        }}
      >
        <label>
          Ou use o seu
          <input value={typed} onChange={(event) => setTyped(event.target.value)} autoComplete="off" placeholder="8 a 200 caracteres, sem espaços" />
        </label>
        <Button type="submit" variant="ghost" className="btn-small" disabled={typed.trim() === "" || saving}>
          Usar este segredo
        </Button>
      </form>
    </>
  );
}

/** Changes where, what and whether. */
function EditPanel({ companyId, subscription, onSaved }: { companyId: string; subscription: WebhookSubscription; onSaved: () => void }) {
  const [url, setUrl] = useState(subscription.url);
  const [events, setEvents] = useState(subscription.events);
  const [active, setActive] = useState(subscription.active);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save(event: FormEvent) {
    event.preventDefault();
    if (saving) return;
    setSaving(true);
    setError(null);
    try {
      await api.updateWebhook(companyId, subscription.id, { url: url.trim(), events, active });
      onSaved();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <form onSubmit={save}>
      <label>
        Endereço
        <input value={url} onChange={(event) => setUrl(event.target.value)} required maxLength={2000} />
      </label>
      <div className="field-grid">
        {Object.keys(EVENT_LABELS)
          .filter((kind) => kind !== "webhook.test")
          .map((kind) => (
            <label key={kind} className="check">
              <input
                type="checkbox"
                checked={events.includes(kind)}
                onChange={(event) => setEvents(event.target.checked ? [...events, kind] : events.filter((other) => other !== kind))}
              />
              {EVENT_LABELS[kind]}
            </label>
          ))}
      </div>
      <label className="check">
        <input type="checkbox" checked={active} onChange={(event) => setActive(event.target.checked)} />
        Ativo
      </label>
      <Notice message={error} />
      <Button type="submit" className="btn-small" pending={saving}>
        Salvar
      </Button>
    </form>
  );
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function WebhooksPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("members");
  const { openCreate } = useCreation();
  const [subscriptions, setSubscriptions] = useState<WebhookSubscription[]>([]);
  const [available, setAvailable] = useState(true);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [open, setOpen] = useState<{ id: string; panel: Panel } | null>(null);
  const [testingId, setTestingId] = useState<string | null>(null);
  // A secret made by replacing it is shown here, once.
  const [made, setMade] = useState<string | null>(null);
  // Bumped when a test is sent, so an open log reads again.
  const [sent, setSent] = useState(0);

  const load = useCallback(async () => {
    setError(null);
    try {
      const reply = await api.listWebhooks(companyId);
      setSubscriptions(reply.items);
      setAvailable(reply.available);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setLoading(false);
    }
  }, [companyId]);

  useEffect(() => {
    void load();
  }, [load]);
  useOnCreated("webhook", () => void load());

  async function test(subscription: WebhookSubscription) {
    if (testingId !== null) return;
    setTestingId(subscription.id);
    setError(null);
    setNotice(null);
    try {
      await api.testWebhook(companyId, subscription.id);
      setNotice("Evento de teste na fila: ele aparece no log em instantes.");
      setSent((held) => held + 1);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setTestingId(null);
    }
  }

  const toggle = (id: string, panel: Panel) => setOpen(open?.id === id && open.panel === panel ? null : { id, panel });

  return (
    <>
      <PageHeader
        title="Webhooks"
        description="Avisos que o sistema envia a um endereço seu quando algo acontece (um pedido, uma cobrança paga, o estoque abaixo do mínimo). Cada envio é assinado com o segredo do webhook, repetido por até cerca de nove horas se o destino não responder, e fica no log."
      />
      {!available && (
        <Notice message="Webhooks desligados: o servidor não tem a chave para guardar os segredos (FISCAL_SECRETS_KEY). Peça a quem cuida do servidor." />
      )}
      {canWrite && available && (
        <div className="list-actions">
          <Button type="button" icon={SendIcon} onClick={() => openCreate("webhook")}>
            Novo webhook
          </Button>
        </div>
      )}
      {made && <WebhookSecret secret={made} title="Novo segredo" />}
      <section className="card list-card">
        <h2>Destinos</h2>
        <Notice message={error} />
        <Notice tone="success" message={notice} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        ) : subscriptions.length === 0 ? (
          <p className="muted">Nenhum webhook ainda.</p>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Endereço</th>
                  <th>Eventos</th>
                  <th>Estado</th>
                  <th>
                    <span className="visually-hidden">Ações</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {subscriptions.map((subscription) => (
                  <Fragment key={subscription.id}>
                    <tr>
                      <td>
                        {subscription.url}
                        <br />
                        <span className="muted">criado por {subscription.created_by}</span>
                      </td>
                      <td>{eventsText(subscription.events)}</td>
                      <td>
                        <span className={`badge ${subscription.active ? "badge-ok" : "badge-muted"}`}>{subscription.active ? "Ativo" : "Desligado"}</span>
                      </td>
                      <td>
                        <span className="form-actions">
                          <Button type="button" variant="ghost" className="btn-small" aria-label={`Log de ${subscription.url}`} onClick={() => toggle(subscription.id, "log")}>
                            Log
                          </Button>
                          {canWrite && (
                            <>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={`Testar ${subscription.url}`}
                                pending={testingId === subscription.id}
                                disabled={testingId !== null}
                                onClick={() => void test(subscription)}
                              >
                                Enviar teste
                              </Button>
                              <Button type="button" variant="ghost" className="btn-small" aria-label={`Segredo de ${subscription.url}`} onClick={() => toggle(subscription.id, "secret")}>
                                Segredo
                              </Button>
                              <Button type="button" variant="ghost" className="btn-small" aria-label={`Editar ${subscription.url}`} onClick={() => toggle(subscription.id, "edit")}>
                                Editar
                              </Button>
                            </>
                          )}
                          <Button type="button" variant="ghost" className="btn-small" aria-label={`Histórico de ${subscription.url}`} onClick={() => toggle(subscription.id, "history")}>
                            Histórico
                          </Button>
                        </span>
                      </td>
                    </tr>
                    {open?.id === subscription.id && (
                      <tr>
                        <td colSpan={4}>
                          {open.panel === "log" && <DeliveryLog companyId={companyId} subscription={subscription} canWrite={canWrite} refresh={sent} />}
                          {open.panel === "secret" && <SecretPanel companyId={companyId} subscription={subscription} onMade={setMade} />}
                          {open.panel === "edit" && (
                            <EditPanel
                              companyId={companyId}
                              subscription={subscription}
                              onSaved={() => {
                                setOpen(null);
                                void load();
                              }}
                            />
                          )}
                          {open.panel === "history" && <WebhookHistory companyId={companyId} id={subscription.id} />}
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
