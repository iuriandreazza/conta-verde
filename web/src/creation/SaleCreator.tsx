import { useEffect, useState } from "react";
import { api, errorMessage, type SaleInput } from "../api";
import { SaleForm } from "../components/SaleForm";
import { Spinner } from "../components/Spinner";
import { useSalesTargets } from "../lib/useSalesTargets";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new order, with what it points at read for the company, and the call that saves it. */
export function SaleCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const targets = useSalesTargets(companyId);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: SaleInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createSale(companyId, input);
      onCreated({ id: created.id, message: `Pedido nº ${created.number} criado. Conclua para gerar a conta a receber.` });
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
    <SaleForm companyId={companyId} sale={null} targets={targets} pending={saving} error={error} onSubmit={(input) => void save(input)} onCancel={onCancel} />
  );
}
