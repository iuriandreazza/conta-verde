import { useEffect, useState, type FormEvent } from "react";
import { api, errorMessage } from "../api";
import { BackButton } from "../components/BackButton";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { WebhookSecret } from "../components/WebhookSecret";
import { SendIcon } from "../components/icons/send";
import { useT } from "../i18n";
import { EVENT_LABELS } from "../lib/webhooks";
import type { CreatorProps } from "./CreationProvider";

const SUBSCRIBABLE = ["sale.created", "sale.completed", "sale.cancelled", "charge.paid", "stock.below_minimum"];

/** The form that subscribes an address to the company's events; the secret that signs them is shown once, at the end. */
export function WebhookCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const t = useT();
  const [chosen, setChosen] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) return;
    const url = String(new FormData(event.currentTarget).get("url") ?? "").trim();
    setSaving(true);
    setError(null);
    try {
      const made = await api.createWebhook(companyId, { url, events: chosen });
      onCreated({
        id: made.subscription.id,
        message: t("Webhook created."),
        result: <WebhookSecret secret={made.secret} title={t("Webhook created")} />,
      });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="webhook-info">
        <h3 id="webhook-info">{t("Where to")}</h3>
        <label>
          {t("Address (https)")}
          <input name="url" type="url" required maxLength={2000} autoComplete="off" placeholder={t("https://my-store.example.com/webhooks")} />
        </label>
      </section>
      <section className="form-section" aria-labelledby="webhook-events">
        <h3 id="webhook-events">{t("Which events")}</h3>
        <div className="field-grid">
          {SUBSCRIBABLE.map((kind) => (
            <label key={kind} className="check">
              <input
                type="checkbox"
                checked={chosen.includes(kind)}
                onChange={(event) => setChosen(event.target.checked ? [...chosen, kind] : chosen.filter((other) => other !== kind))}
              />
              {t(EVENT_LABELS[kind])}
            </label>
          ))}
        </div>
        <p className="muted">{t("With none ticked, the address receives every event.")}</p>
      </section>
      <Notice message={error} />
      <div className="form-footer">
        <BackButton disabled={saving} onClick={onCancel} />
        <Button type="submit" icon={SendIcon} pending={saving}>
          {t("Create webhook")}
        </Button>
      </div>
    </form>
  );
}
