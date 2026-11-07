import { useCallback, useState } from "react";

export interface ColumnDef<K extends string> {
  key: K;
  label: string;
  /** Always shown: the chooser does not offer it. */
  required?: boolean;
  /** Hidden until the person asks for it. */
  hiddenByDefault?: boolean;
}

const storageKey = (name: string) => `conta-verde.columns.${name}`;

function readHidden<K extends string>(name: string, columns: ColumnDef<K>[]): Set<K> {
  const optional = new Set(columns.filter((column) => !column.required).map((column) => column.key));
  try {
    const stored = window.localStorage.getItem(storageKey(name));
    if (stored !== null) {
      const keys: unknown = JSON.parse(stored);
      if (Array.isArray(keys)) {
        // Only what is still an optional column of the page: a column that was renamed or made required is forgotten.
        return new Set(keys.filter((key): key is K => typeof key === "string" && optional.has(key as K)));
      }
    }
  } catch {
    // Storage that is blocked or holds something else: the usual columns.
  }
  return new Set(columns.filter((column) => column.hiddenByDefault && !column.required).map((column) => column.key));
}

/**
 * Which columns of a table are shown, kept in this browser under `name` (a per-person convenience, never something the
 * server needs). Required columns are always shown.
 */
export function useColumns<K extends string>(name: string, columns: ColumnDef<K>[]) {
  const [hidden, setHidden] = useState<Set<K>>(() => readHidden(name, columns));

  const save = useCallback(
    (next: Set<K>) => {
      setHidden(next);
      try {
        window.localStorage.setItem(storageKey(name), JSON.stringify([...next]));
      } catch {
        // Not saved: the choice holds until the page is closed.
      }
    },
    [name],
  );

  const isShown = (key: K) => !hidden.has(key);
  const toggle = (key: K) => {
    const next = new Set(hidden);
    if (!next.delete(key)) next.add(key);
    save(next);
  };
  const reset = () => save(new Set(columns.filter((column) => column.hiddenByDefault && !column.required).map((column) => column.key)));

  return { isShown, toggle, reset, hiddenCount: hidden.size };
}
