import { useI18n } from "../i18n";
import { CopyButton } from "./CopyButton";
import { Notice } from "./Notice";

/** What the person must read once a key exists: its secret is shown only now, and the address to use it with. */
export function ApiKeySecret({ name, secret, title }: { name: string; secret: string; title: string }) {
  const { t, rich } = useI18n();
  const base = `${window.location.origin}/api/v1`;
  return (
    <section className="form-section invite-result" aria-labelledby="api-key-done">
      <h3 id="api-key-done">{title}</h3>
      <Notice tone="warning" message={t("Copy the key now: it does not show again. If you lose it, rotate the key to get another.")} />
      <label>
        {t("Key of “{name}”", { name })}
        <input readOnly value={secret} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p>
        <CopyButton text={secret} label={t("Copy the key of {name}", { name })} />
      </p>
      <label>
        {t("API address")}
        <input readOnly value={base} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p className="muted">
        {rich("Send the key with every request, in the header {header}. The documentation is at {link}.", {
          header: <code>Authorization: Bearer …</code>,
          link: (
            <a href="/api/v1/openapi.json" target="_blank" rel="noreferrer">
              /api/v1/openapi.json
            </a>
          ),
        })}
      </p>
    </section>
  );
}
