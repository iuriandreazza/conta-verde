import { useEffect, useState } from "react";
import { api, errorMessage, type InventoryCountInput } from "../api";
import { InventoryCountForm } from "../components/InventoryCountForm";
import { Spinner } from "../components/Spinner";
import { useStockProducts } from "../lib/useStockProducts";
import { useT } from "../i18n";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new inventory count, with the products read first to choose from. */
export function InventoryCountCreator({ companyId, options, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const t = useT();
  const { products, error: loadError, loaded } = useStockProducts(companyId);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: InventoryCountInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createInventoryCount(companyId, input);
      onCreated({ id: created.id, message: t("Inventory count no. {number} created. Apply it to adjust the stock.", { number: created.number }) });
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
    <InventoryCountForm
      count={null}
      products={products}
      viaSheet={options.viaSheet === true}
      pending={saving}
      error={error}
      onSubmit={(input) => void save(input)}
      onCancel={onCancel}
    />
  );
}
