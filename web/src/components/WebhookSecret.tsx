import { useI18n } from "../i18n";
import { CopyButton } from "./CopyButton";
import { Notice } from "./Notice";

/** What the person must read once a secret was made here: it is shown only now. */
export function WebhookSecret({ secret, title }: { secret: string; title: string }) {
  const { t, rich } = useI18n();
  return (
    <section className="form-section invite-result" aria-labelledby="webhook-secret-done">
      <h3 id="webhook-secret-done">{title}</h3>
      <Notice tone="warning" message={t("Copy the secret now: it does not show again. If you lose it, replace the secret.")} />
      <label>
        {t("Signing secret")}
        <input readOnly value={secret} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p>
        <CopyButton text={secret} label={t("Copy the signing secret")} />
      </p>
      <p className="muted">
        {rich(
          "Every delivery carries the headers {id}, {timestamp} and {signature} ({prefix} and the HMAC-SHA256 of {payload} in base64, with the text of this secret as the key, exactly as it is). Verify the signature, refuse timestamps older than five minutes and keep the ids already seen.",
          {
            id: <code>Webhook-Id</code>,
            timestamp: <code>Webhook-Timestamp</code>,
            signature: <code>Webhook-Signature</code>,
            prefix: <code>v1,</code>,
            payload: <code>{t("id.timestamp.body")}</code>,
          },
        )}
      </p>
    </section>
  );
}
