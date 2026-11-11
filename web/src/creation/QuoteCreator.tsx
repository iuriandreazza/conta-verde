import { useEffect, useState } from "react";
import { api, errorMessage, type QuoteInput } from "../api";
import { useT } from "../i18n";
import { QuoteForm } from "../components/QuoteForm";
import { Spinner } from "../components/Spinner";
import { useSalesTargets } from "../lib/useSalesTargets";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new quote, with the customers, products and terms read first, and the call that saves it. */
export function QuoteCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const t = useT();
  const targets = useSalesTargets(companyId);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: QuoteInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createQuote(companyId, input);
      onCreated({ id: created.id, message: t("Quote no. {number} created.", { number: created.number }) });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  if (!targets.loaded) {
    return targets.error === null ? (
      <p role="status" className="status">
        <Spinner /> {t("Loading…")}
      </p>
    ) : (
      <p role="alert">{targets.error}</p>
    );
  }
  return (
    <QuoteForm companyId={companyId} quote={null} targets={targets} pending={saving} error={error} onSubmit={(input) => void save(input)} onCancel={onCancel} />
  );
}
