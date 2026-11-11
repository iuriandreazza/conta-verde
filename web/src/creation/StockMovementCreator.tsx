import { useEffect, useState } from "react";
import { api, errorMessage, type StockMovementInput } from "../api";
import { Spinner } from "../components/Spinner";
import { StockMovementForm } from "../components/StockMovementForm";
import { useStockProducts } from "../lib/useStockProducts";
import { msg, useT } from "../i18n";
import type { CreatorProps } from "./CreationProvider";

const RECORDED = { in: msg("Stock entry recorded."), out: msg("Stock exit recorded."), adjustment: msg("Stock adjustment recorded.") } as const;

/** The form of a movement written by hand, with the products read first to choose from. */
export function StockMovementCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const t = useT();
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
      onCreated({ id: created.id, message: t(RECORDED[created.kind]) });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  if (!loaded) {
    return loadError === null ? (
      <p role="status" className="status">
        <Spinner /> {t("Loading…")}
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
