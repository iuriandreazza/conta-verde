import { useEffect, useState, type FormEvent } from "react";
import { API_KEY_AREAS, api, errorMessage } from "../api";
import { ApiKeySecret } from "../components/ApiKeySecret";
import { BackButton } from "../components/BackButton";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { SendIcon } from "../components/icons/send";
import { AREA_LABELS, EXPIRY_OPTIONS, REACH_LABELS, scopesOf, type Reach } from "../lib/apiKeys";
import type { CreatorProps } from "./CreationProvider";

const NO_ACCESS = Object.fromEntries(API_KEY_AREAS.map((area) => [area, "none"])) as Record<string, Reach>;

/** The form that makes an API key: a name, what it reaches in each area, and how long it lives. */
export function ApiKeyCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const [reach, setReach] = useState<Record<string, Reach>>(NO_ACCESS);
  const [expiry, setExpiry] = useState(0);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) return;
    const name = String(new FormData(event.currentTarget).get("name") ?? "").trim();
    const scopes = scopesOf(reach);
    if (scopes.length === 0) {
      setError("Escolha o que a chave pode fazer em pelo menos uma área.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const issued = await api.createApiKey(companyId, { name, scopes, expires_in_hours: EXPIRY_OPTIONS[expiry].hours });
      onCreated({
        id: issued.key.id,
        message: `Chave “${issued.key.name}” criada.`,
        result: <ApiKeySecret name={issued.key.name} secret={issued.secret} title="Chave criada" />,
      });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="api-key-info">
        <h3 id="api-key-info">Quem usa</h3>
        <div className="field-grid">
          <label>
            Nome
            <input name="name" required maxLength={200} autoComplete="off" placeholder="Loja online" />
          </label>
          <label>
            Validade
            <select value={expiry} onChange={(event) => setExpiry(Number(event.target.value))}>
              {EXPIRY_OPTIONS.map((option, index) => (
                <option key={option.label} value={index}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>
      <section className="form-section" aria-labelledby="api-key-scopes">
        <h3 id="api-key-scopes">O que a chave pode fazer</h3>
        <div className="field-grid">
          {API_KEY_AREAS.map((area) => (
            <label key={area}>
              {AREA_LABELS[area]}
              <select value={reach[area]} onChange={(event) => setReach({ ...reach, [area]: event.target.value as Reach })}>
                {(Object.keys(REACH_LABELS) as Reach[]).map((level) => (
                  <option key={level} value={level}>
                    {REACH_LABELS[level]}
                  </option>
                ))}
              </select>
            </label>
          ))}
        </div>
        <p className="muted">Dê só o que o programa precisa. Uma chave nunca alcança membros, contabilidade nem fiscal.</p>
      </section>
      <Notice message={error} />
      <div className="form-footer">
        <BackButton disabled={saving} onClick={onCancel} />
        <Button type="submit" icon={SendIcon} pending={saving}>
          Criar chave
        </Button>
      </div>
    </form>
  );
}
