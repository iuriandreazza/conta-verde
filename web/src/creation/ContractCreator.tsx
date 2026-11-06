import { useEffect, useState } from "react";
import { api, errorMessage, type ContractInput } from "../api";
import { ContractForm } from "../components/ContractForm";
import { Spinner } from "../components/Spinner";
import { useContractTargets } from "../lib/useContractTargets";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new contract, with what it points at read for the company, and the call that saves it. */
export function ContractCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const targets = useContractTargets(companyId);
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
      onCreated({ id: created.id, message: `${created.name} cadastrado: os ciclos que já venceram foram cobrados.` });
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
