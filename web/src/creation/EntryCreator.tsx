import { useEffect, useState } from "react";
import { api, errorMessage, type EntryInput } from "../api";
import { EntryForm } from "../components/EntryForm";
import { Spinner } from "../components/Spinner";
import { useEntryTargets } from "../lib/useEntryTargets";
import { useT } from "../i18n";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new entry, with what it points at read for the company, and the call that saves it. */
export function EntryCreator({ companyId, options, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const t = useT();
  const targets = useEntryTargets(companyId, { payments: false });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: EntryInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createEntry(companyId, input);
      onCreated({ id: created.id, message: t("{name} added.", { name: created.description }) });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  if (!targets.loaded) {
    // The choices are read before the form shows, so nothing typed is lost when they arrive.
    return targets.error === null ? (
      <p role="status" className="status">
        <Spinner /> {t("Loading…")}
      </p>
    ) : (
      <p role="alert">{targets.error}</p>
    );
  }
  return (
    <EntryForm
      defaultKind={options.kind}
      entry={null}
      targets={targets}
      pending={saving}
      error={error}
      onSubmit={(input) => void save(input)}
      onCancel={onCancel}
    />
  );
}
