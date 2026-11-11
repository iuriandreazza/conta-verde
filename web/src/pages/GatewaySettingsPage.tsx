import { useCallback, useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type GatewaySettings, type GatewaySettingsInput, type SecretChange } from "../api";
import { Button } from "../components/Button";
import { CopyButton } from "../components/CopyButton";
import { HelpTitle } from "../components/HelpTitle";
import { History } from "../components/History";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { msg, useT } from "../i18n";
import { useCanWrite } from "../lib/useCanWrite";

type SecretName = "focus_token_homologation" | "focus_token_production" | "nfeio_api_key" | "webhook_secret";

/** What is typed for one secret: nothing keeps the stored one, text replaces it, and the box clears it. */
interface SecretField {
  typed: string;
  clear: boolean;
}

const EMPTY_SECRET: SecretField = { typed: "", clear: false };

const FIELD_LABELS: Record<string, string> = {
  kind: "Gateway",
  focus_token_homologation: msg("Focus NFe token (test)"),
  focus_token_production: msg("Focus NFe token (production)"),
  nfeio_api_key: msg("NFE.io API key"),
  webhook_secret: msg("Webhook secret"),
  nfeio_company_homologation: msg("NFE.io company (test)"),
  nfeio_company_production: msg("NFE.io company (production)"),
  nfeio_search_confirmed: msg("Search by reference confirmed"),
};

const VALUE_LABELS: Record<string, string> = {
  set: msg("set"),
  changed: msg("changed"),
  focus: "Focus NFe",
  nfeio: "NFE.io",
  true: msg("yes"),
  false: msg("no"),
};

function change(field: SecretField): SecretChange {
  if (field.typed.trim() !== "") return { action: "set", value: field.typed.trim() };
  return field.clear ? { action: "clear" } : { action: "keep" };
}

/**
 * How this company reaches its fiscal gateway (ADR 0085): which gateway and the secrets to use it. Only the owner and the admin
 * reach it. **A secret is written and never shown**: the page says whether each is set, and what is typed replaces it. The
 * server's own settings (its environment) are used while the company has set none.
 */
export function GatewaySettingsPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("members");
  const [settings, setSettings] = useState<GatewaySettings | null>(null);
  const [kind, setKind] = useState<"focus" | "nfeio">("focus");
  const [secrets, setSecrets] = useState<Record<SecretName, SecretField>>({
    focus_token_homologation: EMPTY_SECRET,
    focus_token_production: EMPTY_SECRET,
    nfeio_api_key: EMPTY_SECRET,
    webhook_secret: EMPTY_SECRET,
  });
  const [homologationCompany, setHomologationCompany] = useState("");
  const [productionCompany, setProductionCompany] = useState("");
  const [searchConfirmed, setSearchConfirmed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [pending, setPending] = useState(false);
  const [version, setVersion] = useState(0);

  const show = useCallback((found: GatewaySettings) => {
    setSettings(found);
    setKind(found.kind ?? "focus");
    setHomologationCompany(found.nfeio_company_homologation ?? "");
    setProductionCompany(found.nfeio_company_production ?? "");
    setSearchConfirmed(found.nfeio_search_confirmed);
    // What was typed is gone as soon as it is saved: it is never kept in the page.
    setSecrets({
      focus_token_homologation: EMPTY_SECRET,
      focus_token_production: EMPTY_SECRET,
      nfeio_api_key: EMPTY_SECRET,
      webhook_secret: EMPTY_SECRET,
    });
  }, []);

  useEffect(() => {
    let current = true;
    api
      .getGatewaySettings(companyId)
      .then((found) => current && show(found))
      .catch((failure) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId, show]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    setSaved(false);
    try {
      const input: GatewaySettingsInput = {
        kind,
        // What belongs to the gateway that is not chosen is not sent: a token typed there and forgotten is not sealed and saved.
        focus_token_homologation: kind === "focus" ? change(secrets.focus_token_homologation) : { action: "keep" },
        focus_token_production: kind === "focus" ? change(secrets.focus_token_production) : { action: "keep" },
        nfeio_api_key: kind === "nfeio" ? change(secrets.nfeio_api_key) : { action: "keep" },
        webhook_secret: change(secrets.webhook_secret),
        nfeio_company_homologation: homologationCompany.trim() || null,
        nfeio_company_production: productionCompany.trim() || null,
        nfeio_search_confirmed: searchConfirmed,
      };
      show(await api.saveGatewaySettings(companyId, input));
      setSaved(true);
      setVersion((current) => current + 1);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setPending(false);
    }
  }

  async function reset() {
    if (!window.confirm(t("Go back to the server's gateway? This company's secrets will be erased."))) return;
    setPending(true);
    setError(null);
    setSaved(false);
    try {
      show(await api.resetGatewaySettings(companyId));
      setVersion((current) => current + 1);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setPending(false);
    }
  }

  const loadHistory = useCallback(() => api.gatewaySettingsHistory(companyId), [companyId, version]);

  if (!settings) {
    return error ? (
      <Notice message={error} />
    ) : (
      <p role="status" className="status">
        <Spinner /> {t("Loading…")}
      </p>
    );
  }

  const isSet: Record<SecretName, boolean> = {
    focus_token_homologation: settings.focus_token_homologation_set,
    focus_token_production: settings.focus_token_production_set,
    nfeio_api_key: settings.nfeio_api_key_set,
    webhook_secret: settings.webhook_secret_set,
  };
  const webhookUrl = `${window.location.origin}${settings.webhook_path}`;

  const secretInput = (name: SecretName, label: string) => (
    <div key={name}>
      <label>
        {label}
        <input
          type="password"
          autoComplete="new-password"
          spellCheck={false}
          value={secrets[name].typed}
          disabled={!canWrite || secrets[name].clear}
          placeholder={isSet[name] ? t("Set: type to replace") : t("Not set")}
          onChange={(event) => setSecrets({ ...secrets, [name]: { ...secrets[name], typed: event.target.value } })}
        />
      </label>
      {isSet[name] && canWrite && (
        <label className="inline-check">
          <input
            type="checkbox"
            checked={secrets[name].clear}
            onChange={(event) => setSecrets({ ...secrets, [name]: { typed: "", clear: event.target.checked } })}
            aria-label={t("Remove: {label}", { label })}
          />{" "}
          {t("Remove")}
        </label>
      )}
    </div>
  );

  return (
    <>
      <PageHeader
        title={t("Fiscal gateway")}
        description={t(
          "Which gateway this company issues its fiscal documents through, and the secrets to use it. Only the owner and the administrator get here. A secret is stored encrypted and never shown again: the screen only says whether it is set, and what you type replaces it. While the company sets nothing, what the server has in its environment applies. If what the company sets is not enough for the environment in use (the token or the key is missing), issuing is refused saying what is missing: the server's gateway never takes its place.",
        )}
      />
      <Notice message={error} />
      {saved && <p role="status">{t("Saved.")}</p>}
      {!settings.encryption_available && (
        <Notice
          tone="warning"
          message={t(
            "The server has no encryption key (FISCAL_SECRETS_KEY): no secret can be saved until the operator sets it. The gateway can be chosen anyway.",
          )}
        />
      )}
      {settings.unreadable.length > 0 && (
        <Notice
          tone="warning"
          message={t(
            "These secrets are saved, but the server cannot read them (the encryption key changed or was lost): {fields}. Type them again.",
            { fields: settings.unreadable.map((field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)).join(", ") },
          )}
        />
      )}
      {settings.kind === null && settings.server_gateway !== null && (
        <Notice
          tone="warning"
          message={t("The company has not configured the gateway yet: the server's applies ({gateway}).", { gateway: settings.server_gateway })}
        />
      )}
      <form className="record-form" onSubmit={(event) => void submit(event)}>
        <HelpTitle title="Gateway" level={3}>
          <p>
            {t(
              "The A1 certificate is still sent in the gateway's panel, never here. The gateway's address and the hosts files are downloaded from are not set here: they stay on the server.",
            )}
          </p>
        </HelpTitle>
        <div className="field-grid">
          <label>
            Gateway
            <select value={kind} disabled={!canWrite} onChange={(event) => setKind(event.target.value as "focus" | "nfeio")}>
              <option value="focus">Focus NFe</option>
              <option value="nfeio">NFE.io</option>
            </select>
          </label>
          {kind === "focus" && (
            <>
              {secretInput("focus_token_homologation", t(FIELD_LABELS.focus_token_homologation))}
              {secretInput("focus_token_production", t(FIELD_LABELS.focus_token_production))}
            </>
          )}
          {kind === "nfeio" && (
            <>
              {secretInput("nfeio_api_key", t(FIELD_LABELS.nfeio_api_key))}
              <label>
                {t(FIELD_LABELS.nfeio_company_homologation)}
                <input value={homologationCompany} disabled={!canWrite} onChange={(event) => setHomologationCompany(event.target.value)} />
              </label>
              <label>
                {t(FIELD_LABELS.nfeio_company_production)}
                <input value={productionCompany} disabled={!canWrite} onChange={(event) => setProductionCompany(event.target.value)} />
              </label>
              <label>
                <input type="checkbox" checked={searchConfirmed} disabled={!canWrite} onChange={(event) => setSearchConfirmed(event.target.checked)} /> {t(FIELD_LABELS.nfeio_search_confirmed)}
              </label>
            </>
          )}
          {secretInput("webhook_secret", t(FIELD_LABELS.webhook_secret))}
        </div>
        <p>
          {t("Webhook address:")} <code>{webhookUrl}</code> <CopyButton text={webhookUrl} label={t("Copy the webhook address")} />
        </p>
        {canWrite && (
          <div className="form-actions">
            <Button type="submit" pending={pending}>
              {t("Save")}
            </Button>
            {settings.kind !== null && (
              <Button type="button" disabled={pending} onClick={() => void reset()}>
                {t("Back to the server's gateway")}
              </Button>
            )}
          </div>
        )}
      </form>
      <section aria-label={t("History")}>
        <h3>{t("History")}</h3>
        <History
          load={loadHistory}
          fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)}
          valueLabel={(_, value) => (value === null ? "—" : value in VALUE_LABELS ? t(VALUE_LABELS[value]) : value)}
          emptyMessage={t("No records yet.")}
        />
      </section>
    </>
  );
}
