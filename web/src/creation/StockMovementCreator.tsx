import { useEffect, useState } from "react";
import { api, errorMessage, type StockMovementInput } from "../api";
import { Spinner } from "../components/Spinner";
import { StockMovementForm } from "../components/StockMovementForm";
import { useStockProducts } from "../lib/useStockProducts";
import type { CreatorProps } from "./CreationProvider";

const KIND_WORDS = { in: "Entrada", out: "Saída", adjustment: "Ajuste" } as const;

/** The form of a movement written by hand, with the products read first to choose from. */
export function StockMovementCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const { products, error: loadError, loaded } = useStockProducts(companyId);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: StockMovementInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.recordStockMovement(companyId, input);
      onCreated({ id: created.id, message: `${KIND_WORDS[created.kind]} de estoque registrada.` });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  if (!loaded) {
    return loadError === null ? (
      <p role="status" className="status">
        <Spinner /> Carregando…
      </p>
    ) : (
      <p role="alert">{loadError}</p>
    );
  }
  return (
    <StockMovementForm
      products={products.filter((product) => product.active)}
      pending={saving}
      error={error}
      onSubmit={(input) => void save(input)}
      onCancel={onCancel}
    />
  );
}
