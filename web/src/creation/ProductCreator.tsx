import { useEffect, useState } from "react";
import { api, errorMessage, type ProductInput } from "../api";
import { ProductForm } from "../components/ProductForm";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new product or service, and the call that saves it. */
export function ProductCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: ProductInput) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createProduct(companyId, input);
      onCreated({ id: created.id, message: `${created.name} cadastrado.` });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  return (
    <ProductForm
      product={null}
      pending={saving}
      error={error}
      onSubmit={(input) => void save(input)}
      onCancel={onCancel}
    />
  );
}
