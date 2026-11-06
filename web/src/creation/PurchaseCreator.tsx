import { useEffect, useState } from "react";
import { api, errorMessage, type PurchaseOrderInput } from "../api";
import { PurchaseForm } from "../components/PurchaseForm";
import { Spinner } from "../components/Spinner";
import { useSalesTargets } from "../lib/useSalesTargets";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new purchase order, with what it points at read for the company, and the call that saves it. */
export function PurchaseCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const targets = useSalesTargets(companyId);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: PurchaseOrderInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createPurchaseOrder(companyId, input);
      onCreated({ id: created.id, message: `Compra nº ${created.number} criada.` });
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
    <PurchaseForm companyId={companyId} order={null} targets={targets} pending={saving} error={error} onSubmit={(input) => void save(input)} onCancel={onCancel} />
  );
}
