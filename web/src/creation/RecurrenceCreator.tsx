import { useEffect, useState } from "react";
import { api, errorMessage, type RecurrenceInput } from "../api";
import { RecurrenceForm } from "../components/RecurrenceForm";
import { Spinner } from "../components/Spinner";
import { useEntryTargets } from "../lib/useEntryTargets";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new recurrence, with what it points at read for the company, and the call that saves it. */
export function RecurrenceCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const targets = useEntryTargets(companyId, { payments: false });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: RecurrenceInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createRecurrence(companyId, input);
      onCreated({
        id: created.id,
        message: created.active
          ? `${created.description} cadastrada: os lançamentos que vencem nos próximos dois meses já foram gerados.`
          : `${created.description} cadastrada, pausada: nada será gerado até ativá-la.`,
      });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  if (!targets.loaded) {
    return targets.error === null ? (
      <p role="status" className="status">
        <Spinner /> Carregando…
      </p>
    ) : (
      <p role="alert">{targets.error}</p>
    );
  }
  return (
    <RecurrenceForm recurrence={null} targets={targets} pending={saving} error={error} onSubmit={(input) => void save(input)} onCancel={onCancel} />
  );
}
