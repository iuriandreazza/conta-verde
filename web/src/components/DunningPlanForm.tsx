import { useState, type FormEvent } from "react";
import { api, errorMessage, type DunningChannel, type DunningPlan, type DunningRule } from "../api";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { XIcon } from "./icons/x";
import { Notice } from "./Notice";

const CHANNEL_LABELS: Record<DunningChannel, string> = { email: "E-mail", whatsapp: "WhatsApp" };

type When = "before" | "on" | "after";

interface Row {
  key: number;
  when: When;
  count: string;
  channel: DunningChannel;
}

function toRow(rule: DunningRule, key: number): Row {
  return {
    key,
    when: rule.days < 0 ? "before" : rule.days === 0 ? "on" : "after",
    count: String(Math.abs(rule.days)),
    channel: rule.channel,
  };
}

function toRule(row: Row): DunningRule | null {
  const count = Number(row.count);
  if (row.when !== "on" && (!Number.isInteger(count) || count < 1)) {
    return null;
  }
  return {
    days: row.when === "before" ? -count : row.when === "after" ? count : 0,
    channel: row.channel,
  };
}

/** Describes a rule in words, for the list of what was sent. */
export function describeRule(days: number): string {
  if (days === 0) {
    return "no vencimento";
  }
  const unit = Math.abs(days) === 1 ? "dia" : "dias";
  return days < 0 ? `${-days} ${unit} antes` : `${days} ${unit} depois`;
}

interface DunningPlanFormProps {
  companyId: string;
  plan: DunningPlan;
  canWrite: boolean;
  onSaved: (plan: DunningPlan) => void;
}

/**
 * When customers are reminded of an installment, and by which channel (ADR 0048): a switch and a list of
 * rules, saved as a whole. A reminder goes out on the due date moved by the rule's days.
 */
export function DunningPlanForm({ companyId, plan, canWrite, onSaved }: DunningPlanFormProps) {
  const [enabled, setEnabled] = useState(plan.enabled);
  const [rows, setRows] = useState<Row[]>(plan.rules.map(toRow));
  const [nextKey, setNextKey] = useState(plan.rules.length);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  function change(key: number, patch: Partial<Row>) {
    setRows((current) => current.map((row) => (row.key === key ? { ...row, ...patch } : row)));
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const rules = rows.map(toRule);
    if (rules.some((rule) => rule === null)) {
      setError("Informe quantos dias, um número inteiro de 1 em diante, em cada lembrete.");
      return;
    }
    setSaving(true);
    setError(null);
    setSaved(null);
    try {
      const stored = await api.replaceDunningPlan(companyId, {
        enabled,
        rules: rules as DunningRule[],
      });
      onSaved(stored);
      setRows(stored.rules.map(toRow));
      setSaved("Lembretes salvos.");
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <form onSubmit={(event) => void handleSubmit(event)} className="settle-form">
      <h2>Lembretes aos clientes</h2>
      <p className="muted">
        A cada hora o sistema lembra o cliente das parcelas a receber que ainda têm saldo: uma vez por regra,
        pelo canal escolhido, com o link para pagar quando há cobrança aberta.
      </p>
      <Notice message={error} />
      <Notice message={saved} tone="success" />
      <label className="inline">
        <input
          type="checkbox"
          checked={enabled}
          disabled={!canWrite}
          onChange={(event) => setEnabled(event.target.checked)}
        />{" "}
        Enviar lembretes
      </label>
      {rows.map((row, index) => (
        <div key={row.key} className="rule-row">
          <label>
            Quando
            <select
              aria-label={`Quando, lembrete ${index + 1}`}
              value={row.when}
              disabled={!canWrite}
              onChange={(event) => change(row.key, { when: event.target.value as When })}
            >
              <option value="before">Antes do vencimento</option>
              <option value="on">No vencimento</option>
              <option value="after">Depois do vencimento</option>
            </select>
          </label>
          {row.when !== "on" && (
            <label>
              Dias
              <input
                aria-label={`Dias, lembrete ${index + 1}`}
                inputMode="numeric"
                value={row.count}
                disabled={!canWrite}
                onChange={(event) => change(row.key, { count: event.target.value })}
              />
            </label>
          )}
          <label>
            Canal
            <select
              aria-label={`Canal, lembrete ${index + 1}`}
              value={row.channel}
              disabled={!canWrite}
              onChange={(event) => change(row.key, { channel: event.target.value as DunningChannel })}
            >
              {(Object.keys(CHANNEL_LABELS) as DunningChannel[])
                .filter((channel) => channel !== "whatsapp" || plan.whatsapp_available || row.channel === "whatsapp")
                .map((channel) => (
                  <option key={channel} value={channel}>
                    {CHANNEL_LABELS[channel]}
                  </option>
                ))}
            </select>
          </label>
          {canWrite && (
            <Button
              type="button"
              iconOnly
              icon={XIcon}
              variant="ghost"
              aria-label={`Remover o lembrete ${index + 1}`}
              onClick={() => setRows((current) => current.filter((candidate) => candidate.key !== row.key))}
            >
              Remover
            </Button>
          )}
        </div>
      ))}
      {row_hint(rows.length, plan.whatsapp_available)}
      {canWrite && (
        <div className="form-actions">
          <Button
            type="button"
            variant="ghost"
            icon={PlusIcon}
            onClick={() => {
              setRows((current) => [...current, { key: nextKey, when: "before", count: "3", channel: "email" }]);
              setNextKey(nextKey + 1);
            }}
          >
            Adicionar lembrete
          </Button>
          <Button type="submit" icon={CheckIcon} pending={saving}>
            Salvar lembretes
          </Button>
        </div>
      )}
    </form>
  );
}

function row_hint(count: number, whatsappAvailable: boolean) {
  return (
    <>
      {count === 0 && <p className="muted">Nenhum lembrete: adicione um, por exemplo 3 dias antes por e-mail.</p>}
      {!whatsappAvailable && (
        <p className="muted">WhatsApp não está configurado nesta instalação: só o e-mail é enviado.</p>
      )}
    </>
  );
}
