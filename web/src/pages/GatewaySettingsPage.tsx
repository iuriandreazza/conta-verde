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
  focus_token_homologation: "Token do Focus NFe (homologação)",
  focus_token_production: "Token do Focus NFe (produção)",
  nfeio_api_key: "Chave da API do NFE.io",
  webhook_secret: "Segredo do webhook",
  nfeio_company_homologation: "Empresa no NFE.io (homologação)",
  nfeio_company_production: "Empresa no NFE.io (produção)",
  nfeio_search_confirmed: "Busca por referência confirmada",
};

const VALUE_LABELS: Record<string, string> = { set: "configurado", changed: "trocado", focus: "Focus NFe", nfeio: "NFE.io", true: "sim", false: "não" };

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
    if (!window.confirm("Voltar ao gateway do servidor? Os segredos desta empresa serão apagados.")) return;
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
        <Spinner /> Carregando…
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
          placeholder={isSet[name] ? "Configurado: digite para trocar" : "Não configurado"}
          onChange={(event) => setSecrets({ ...secrets, [name]: { ...secrets[name], typed: event.target.value } })}
        />
      </label>
      {isSet[name] && canWrite && (
        <label className="inline-check">
          <input
            type="checkbox"
            checked={secrets[name].clear}
            onChange={(event) => setSecrets({ ...secrets, [name]: { typed: "", clear: event.target.checked } })}
            aria-label={`Remover: ${label}`}
          />{" "}
          Remover
        </label>
      )}
    </div>
  );

  return (
    <>
      <PageHeader
        title="Gateway fiscal"
        description="Por qual gateway esta empresa emite os documentos fiscais e os segredos para usá-lo. Só o dono e o administrador chegam aqui. Um segredo é gravado cifrado e nunca é mostrado de novo: a tela só diz se ele está configurado, e o que você digita o substitui. Enquanto a empresa não configura nada, vale o que o servidor tem no ambiente. Se o que a empresa configura não basta para o ambiente em uso (falta o token ou a chave), a emissão é recusada dizendo o que falta: o gateway do servidor nunca entra no lugar."
      />
      <Notice message={error} />
      {saved && <p role="status">Salvo.</p>}
      {!settings.encryption_available && (
        <Notice
          tone="warning"
          message="O servidor não tem a chave de cifragem (FISCAL_SECRETS_KEY): nenhum segredo pode ser salvo até o operador defini-la. O gateway pode ser escolhido mesmo assim."
        />
      )}
      {settings.unreadable.length > 0 && (
        <Notice
          tone="warning"
          message={`Estes segredos estão salvos, mas o servidor não consegue lê-los (a chave de cifragem mudou ou foi perdida): ${settings.unreadable
            .map((field) => FIELD_LABELS[field] ?? field)
            .join(", ")}. Digite-os de novo.`}
        />
      )}
      {settings.kind === null && settings.server_gateway !== null && (
        <Notice tone="warning" message={`A empresa ainda não configurou o gateway: vale o do servidor (${settings.server_gateway}).`} />
      )}
      <form className="record-form" onSubmit={(event) => void submit(event)}>
        <HelpTitle title="Gateway" level={3}>
          <p>
            O certificado A1 continua sendo enviado no painel do gateway, nunca aqui. O endereço do gateway e os hosts de onde os arquivos
            são baixados não se configuram aqui: ficam no servidor.
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
              {secretInput("focus_token_homologation", FIELD_LABELS.focus_token_homologation)}
              {secretInput("focus_token_production", FIELD_LABELS.focus_token_production)}
            </>
          )}
          {kind === "nfeio" && (
            <>
              {secretInput("nfeio_api_key", FIELD_LABELS.nfeio_api_key)}
              <label>
                {FIELD_LABELS.nfeio_company_homologation}
                <input value={homologationCompany} disabled={!canWrite} onChange={(event) => setHomologationCompany(event.target.value)} />
              </label>
              <label>
                {FIELD_LABELS.nfeio_company_production}
                <input value={productionCompany} disabled={!canWrite} onChange={(event) => setProductionCompany(event.target.value)} />
              </label>
              <label>
                <input type="checkbox" checked={searchConfirmed} disabled={!canWrite} onChange={(event) => setSearchConfirmed(event.target.checked)} /> {FIELD_LABELS.nfeio_search_confirmed}
              </label>
            </>
          )}
          {secretInput("webhook_secret", FIELD_LABELS.webhook_secret)}
        </div>
        <p>
          Endereço do webhook: <code>{webhookUrl}</code> <CopyButton text={webhookUrl} label="Copiar o endereço do webhook" />
        </p>
        {canWrite && (
          <div className="form-actions">
            <Button type="submit" pending={pending}>
              Salvar
            </Button>
            {settings.kind !== null && (
              <Button type="button" disabled={pending} onClick={() => void reset()}>
                Voltar ao gateway do servidor
              </Button>
            )}
          </div>
        )}
      </form>
      <section aria-label="Histórico">
        <h3>Histórico</h3>
        <History
          load={loadHistory}
          fieldLabel={(field) => FIELD_LABELS[field] ?? field}
          valueLabel={(_, value) => (value === null ? "—" : (VALUE_LABELS[value] ?? value))}
          emptyMessage="Ainda não há registros."
        />
      </section>
    </>
  );
}
