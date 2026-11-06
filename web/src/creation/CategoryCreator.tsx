import { useEffect, useState } from "react";
import { api, errorMessage, type Category, type NewCategory } from "../api";
import { CategoryForm } from "../components/CategoryForm";
import { Spinner } from "../components/Spinner";
import type { CreatorProps } from "./CreationProvider";

/** The form of a new category or subcategory, with the plan read first to choose the parent from. */
export function CategoryCreator({ companyId, options, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const [categories, setCategories] = useState<Category[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    api
      .listCategories(companyId)
      .then((list) => current && setCategories(list.items))
      .catch((failure: unknown) => current && setLoadError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  useEffect(() => {
    onBusyChange(saving);
  }, [saving, onBusyChange]);

  async function save(input: NewCategory) {
    setSaving(true);
    setError(null);
    try {
      const created = await api.createCategory(companyId, input);
      onCreated({ id: created.id, message: `${created.name} cadastrada.` });
    } catch (failure) {
      setError(errorMessage(failure));
      setSaving(false);
    }
  }

  if (categories === null) {
    return loadError === null ? (
      <p role="status" className="status">
        <Spinner /> Carregando…
      </p>
    ) : (
      <p role="alert">{loadError}</p>
    );
  }
  return (
    <CategoryForm
      categories={categories}
      editing={null}
      // The parent was chosen from the page's list: another tab may have changed the plan since.
      initialParentId={categories.some((category) => category.id === options.parentId) ? (options.parentId ?? null) : null}
      pending={saving}
      error={error}
      onCreate={(input) => void save(input)}
      onChange={() => undefined}
      onCancel={onCancel}
    />
  );
}
