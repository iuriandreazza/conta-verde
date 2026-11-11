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
import { useT } from "../i18n";
import { DELIVERY_LABELS, EVENT_LABELS, eventLabel, eventsText } from "../lib/webhooks";
import { useCanWrite } from "../lib/useCanWrite";

type Panel = "log" | "secret" | "edit" | "history";

const BADGES: Record<WebhookDelivery["status"], string> = { pending: "badge-warning", delivered: "badge-ok", failed: "badge-critical" };

const formatMoment = (millis: number) => new Date(millis).toLocaleString("pt-BR");

/** The log of what was sent to one subscription, newest first, with the way to send one again. */
function DeliveryLog({ companyId, subscription, canWrite, refresh }: { companyId: string; subscription: WebhookSubscription; canWrite: boolean; refresh: number }) {
  const t = useT();
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
        <Spinner size={16} /> {t("Loading deliveries…")}
      </p>
    );
  }
  return (
    <>
      <Notice message={error} />
      {items !== null && items.length === 0 && <p className="muted">{t("No deliveries yet.")}</p>}
      {items !== null && items.length > 0 && (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>{t("Event")}</th>
                <th>{t("Status")}</th>
                <th>{t("Attempts")}</th>
                <th>{t("Last response")}</th>
                <th>{t("When")}</th>
                <th>
                  <span className="visually-hidden">{t("Actions")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {items.map((delivery) => (
                <tr key={delivery.id}>
                  <td>{eventLabel(delivery.kind, t)}</td>
                  <td>
                    <span className={`badge ${BADGES[delivery.status]}`}>{t(DELIVERY_LABELS[delivery.status])}</span>
                  </td>
                  <td>{delivery.attempts}</td>
                  <td>{delivery.last_error ?? (delivery.last_status ? t("Response {status}", { status: delivery.last_status }) : "—")}</td>
                  <td>{formatMoment(delivery.created_at)}</td>
                  <td>
                    {/* A pending one has a retry of its own: a second would send it twice. */}
                    {canWrite && delivery.status !== "pending" && (
                      <Button
                        type="button"
                        variant="ghost"
                        className="btn-small"
                        aria-label={t("Send again: {event} of {when}", { event: eventLabel(delivery.kind, t), when: formatMoment(delivery.created_at) })}
                        pending={busy === delivery.id}
                        disabled={busy !== null}
                        onClick={() => void again(delivery)}
                      >
                        {t("Send again")}
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
          {t("See more")}
        </Button>
      )}
      <p className="muted">{t("We keep the deliveries for 30 days. Only the response code and a short reason are recorded, never what the destination answered.")}</p>
    </>
  );
}

/** Replaces the secret: with one of the person's own, or with one made here and shown once. */
function SecretPanel({ companyId, subscription, onMade }: { companyId: string; subscription: WebhookSubscription; onMade: (secret: string) => void }) {
  const t = useT();
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
        setDone(t("Secret replaced. The previous one keeps signing for 24 hours."));
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
        {t("The secret is kept encrypted and is never shown again. When you replace it, the previous one keeps signing for 24 hours, so whoever receives can switch without a gap.")}
      </p>
      <Notice message={error} />
      <Notice tone="success" message={done} />
      <div className="form-actions">
        <Button type="button" className="btn-small" pending={saving} onClick={() => void replace()}>
          {t("Generate a new secret")}
        </Button>
      </div>
      <form
        className="form-actions"
        onSubmit={(event: FormEvent) => {
          event.preventDefault();
          void replace(typed.trim());
        }}
      >
        <label>
          {t("Or use your own")}
          <input value={typed} onChange={(event) => setTyped(event.target.value)} autoComplete="off" placeholder={t("8 to 200 characters, no spaces")} />
        </label>
        <Button type="submit" variant="ghost" className="btn-small" disabled={typed.trim() === "" || saving}>
          {t("Use this secret")}
        </Button>
      </form>
    </>
  );
}

/** Changes where, what and whether. */
function EditPanel({ companyId, subscription, onSaved }: { companyId: string; subscription: WebhookSubscription; onSaved: () => void }) {
  const t = useT();
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
        {t("Address")}
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
              {t(EVENT_LABELS[kind])}
            </label>
          ))}
      </div>
      <label className="check">
        <input type="checkbox" checked={active} onChange={(event) => setActive(event.target.checked)} />
        {t("Active")}
      </label>
      <Notice message={error} />
      <Button type="submit" className="btn-small" pending={saving}>
        {t("Save")}
      </Button>
    </form>
  );
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function WebhooksPage() {
  const t = useT();
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
      setNotice(t("Test event queued: it shows in the log in a moment."));
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
        title={t("Webhooks")}
        description={t("Notices the system sends to an address of yours when something happens (an order, a paid charge, stock below the minimum). Each delivery is signed with the webhook's secret, retried for up to about nine hours if the destination does not answer, and kept in the log.")}
      />
      {!available && (
        <Notice message={t("Webhooks are off: the server has no key to keep the secrets (FISCAL_SECRETS_KEY). Ask whoever looks after the server.")} />
      )}
      {canWrite && available && (
        <div className="list-actions">
          <Button type="button" icon={SendIcon} onClick={() => openCreate("webhook")}>
            {t("New webhook")}
          </Button>
        </div>
      )}
      {made && (
        <>
          <WebhookSecret secret={made} title={t("New secret")} />
          <Button type="button" variant="ghost" className="btn-small" onClick={() => setMade(null)}>
            {t("I copied the secret")}
          </Button>
        </>
      )}
      <section className="card list-card">
        <h2>{t("Destinations")}</h2>
        <Notice message={error} />
        <Notice tone="success" message={notice} />
        {loading ? (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        ) : subscriptions.length === 0 ? (
          <p className="muted">{t("No webhooks yet.")}</p>
        ) : (
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>{t("Address")}</th>
                  <th>{t("Events")}</th>
                  <th>{t("Status")}</th>
                  <th>
                    <span className="visually-hidden">{t("Actions")}</span>
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
                        <span className="muted">{t("created by {name}", { name: subscription.created_by })}</span>
                      </td>
                      <td>{eventsText(subscription.events, t)}</td>
                      <td>
                        <span className={`badge ${subscription.active ? "badge-ok" : "badge-muted"}`}>{subscription.active ? t("Active") : t("Turned off")}</span>
                      </td>
                      <td>
                        <span className="form-actions">
                          <Button type="button" variant="ghost" className="btn-small" aria-label={t("Log of {url}", { url: subscription.url })} onClick={() => toggle(subscription.id, "log")}>
                            {t("Log")}
                          </Button>
                          {canWrite && (
                            <>
                              <Button
                                type="button"
                                variant="ghost"
                                className="btn-small"
                                aria-label={t("Test {url}", { url: subscription.url })}
                                pending={testingId === subscription.id}
                                disabled={testingId !== null}
                                onClick={() => void test(subscription)}
                              >
                                {t("Send test")}
                              </Button>
                              <Button type="button" variant="ghost" className="btn-small" aria-label={t("Secret of {url}", { url: subscription.url })} onClick={() => toggle(subscription.id, "secret")}>
                                {t("Secret")}
                              </Button>
                              <Button type="button" variant="ghost" className="btn-small" aria-label={t("Edit {url}", { url: subscription.url })} onClick={() => toggle(subscription.id, "edit")}>
                                {t("Edit")}
                              </Button>
                            </>
                          )}
                          <Button type="button" variant="ghost" className="btn-small" aria-label={t("History of {url}", { url: subscription.url })} onClick={() => toggle(subscription.id, "history")}>
                            {t("History")}
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
