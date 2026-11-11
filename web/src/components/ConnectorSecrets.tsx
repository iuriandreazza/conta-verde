import { useI18n } from "../i18n";
import { CopyButton } from "./CopyButton";
import { Notice } from "./Notice";

/** What the person must read once a connector was turned on: the two secrets exist only now, and where to put them. */
export function ConnectorSecrets({ name, apiKey, webhookSecret, packageLink }: { name: string; apiKey: string; webhookSecret: string; packageLink: string }) {
  const { t, rich } = useI18n();
  const base = `${window.location.origin}/api/v1`;
  return (
    <section className="form-section invite-result" aria-labelledby="connector-done">
      <h3 id="connector-done">{t("“{name}” turned on", { name })}</h3>
      <Notice tone="warning" message={t("Copy the key and the secret now: they do not show again. If you lose them, turn the connector off and on.")} />
      <label>
        {t("API key")}
        <input readOnly value={apiKey} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p>
        <CopyButton text={apiKey} label={t("Copy the key of {name}", { name })} />
      </p>
      <label>
        {t("Webhook secret")}
        <input readOnly value={webhookSecret} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p>
        <CopyButton text={webhookSecret} label={t("Copy the webhook secret of {name}", { name })} />
      </p>
      <label>
        {t("API address")}
        <input readOnly value={base} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p className="muted">
        {rich("Paste the three into the package's configuration, together with the store's data, which stays only there. The step by step is in the {link}.", {
          link: (
            <a href={packageLink} target="_blank" rel="noreferrer">
              {t("package README")}
            </a>
          ),
        })}
      </p>
    </section>
  );
}
