import { useEffect, useState } from "react";
import { api, errorMessage, type ContractInput } from "../api";
import { useT } from "../i18n";
import { ContractForm } from "../components/ContractForm";
import { Spinner } from "../components/Spinner";
import { useSalesTargets } from "../lib/useSalesTargets";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new contract, with what it points at read for the company, and the call that saves it. */
export function ContractCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const t = useT();
  const targets = useSalesTargets(companyId);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: ContractInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createContract(companyId, input);
      onCreated({ id: created.id, message: t("{name} registered: the cycles already due were billed.", { name: created.name }) });
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
    <ContractForm
      companyId={companyId}
      contract={null}
      targets={targets}
      pending={saving}
      error={error}
      onSubmit={(input) => void save(input)}
      onCancel={onCancel}
    />
  );
}
