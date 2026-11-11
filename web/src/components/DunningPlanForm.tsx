import { useState, type FormEvent } from "react";
import { api, errorMessage, type DunningChannel, type DunningPlan, type DunningRule } from "../api";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { XIcon } from "./icons/x";
import { InfoTip } from "./InfoTip";
import { Notice } from "./Notice";
import { msg, useT, type Translate } from "../i18n";

const CHANNEL_LABELS: Record<DunningChannel, string> = { email: msg("E-mail"), whatsapp: msg("WhatsApp") };

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
export function describeRule(days: number, t: Translate): string {
  if (days === 0) {
    return t("on the due date");
  }
  const count = Math.abs(days);
  if (days < 0) {
    return count === 1 ? t("{count} day before", { count }) : t("{count} days before", { count });
  }
  return count === 1 ? t("{count} day after", { count }) : t("{count} days after", { count });
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
  const t = useT();
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
      setError(t("Give how many days, a whole number from 1 up, in each reminder."));
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
      setSaved(t("Reminders saved."));
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <form onSubmit={(event) => void handleSubmit(event)} className="settle-form">
      <div className="card-title">
        <h2>{t("Reminders to customers")}</h2>
        <InfoTip label={t("How reminders work")}>
          <p>
            {t("Every hour the system reminds the customer of the receivable installments that still have a balance: once per rule, through the chosen channel, with the link to pay when a charge is open.")}
          </p>
        </InfoTip>
      </div>
      <Notice message={error} />
      <Notice message={saved} tone="success" />
      <label className="inline">
        <input
          type="checkbox"
          checked={enabled}
          disabled={!canWrite}
          onChange={(event) => setEnabled(event.target.checked)}
        />{" "}
        {t("Send reminders")}
      </label>
      {rows.map((row, index) => (
        <div key={row.key} className="rule-row">
          <label>
            {t("When")}
            <select
              aria-label={t("When, reminder {number}", { number: index + 1 })}
              value={row.when}
              disabled={!canWrite}
              onChange={(event) => change(row.key, { when: event.target.value as When })}
            >
              <option value="before">{t("Before the due date")}</option>
              <option value="on">{t("On the due date")}</option>
              <option value="after">{t("After the due date")}</option>
            </select>
          </label>
          {row.when !== "on" && (
            <label>
              {t("Days")}
              <input
                aria-label={t("Days, reminder {number}", { number: index + 1 })}
                inputMode="numeric"
                value={row.count}
                disabled={!canWrite}
                onChange={(event) => change(row.key, { count: event.target.value })}
              />
            </label>
          )}
          <label>
            {t("Channel")}
            <select
              aria-label={t("Channel, reminder {number}", { number: index + 1 })}
              value={row.channel}
              disabled={!canWrite}
              onChange={(event) => change(row.key, { channel: event.target.value as DunningChannel })}
            >
              {(Object.keys(CHANNEL_LABELS) as DunningChannel[])
                .filter((channel) => channel !== "whatsapp" || plan.whatsapp_available || row.channel === "whatsapp")
                .map((channel) => (
                  <option key={channel} value={channel}>
                    {t(CHANNEL_LABELS[channel])}
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
              aria-label={t("Remove reminder {number}", { number: index + 1 })}
              onClick={() => setRows((current) => current.filter((candidate) => candidate.key !== row.key))}
            >
              {t("Remove")}
            </Button>
          )}
        </div>
      ))}
      {row_hint(rows.length, plan.whatsapp_available, t)}
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
            {t("Add reminder")}
          </Button>
          <Button type="submit" icon={CheckIcon} pending={saving}>
            {t("Save reminders")}
          </Button>
        </div>
      )}
    </form>
  );
}

function row_hint(count: number, whatsappAvailable: boolean, t: Translate) {
  return (
    <>
      {count === 0 && <p className="muted">{t("No reminders: add one, for example 3 days before by e-mail.")}</p>}
      {!whatsappAvailable && (
        <p className="muted">{t("WhatsApp is not set up in this installation: only e-mail is sent.")}</p>
      )}
    </>
  );
}
