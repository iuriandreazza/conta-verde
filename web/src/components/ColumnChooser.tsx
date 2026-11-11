import { useEffect, useRef } from "react";
import { useT } from "../i18n";
import type { ColumnDef } from "../lib/useColumns";
import { Button } from "./Button";

interface ColumnChooserProps<K extends string> {
  columns: ColumnDef<K>[];
  isShown: (key: K) => boolean;
  onToggle: (key: K) => void;
  onReset: () => void;
}

/** Which columns of a table to show: a button that opens a list of checkboxes, closed by Escape or a click elsewhere. */
export function ColumnChooser<K extends string>({ columns, isShown, onToggle, onReset }: ColumnChooserProps<K>) {
  const t = useT();
  const root = useRef<HTMLDetailsElement>(null);

  useEffect(() => {
    const close = (event: Event) => {
      const details = root.current;
      if (!details?.open) return;
      if (event.type === "keydown" && (event as KeyboardEvent).key !== "Escape") return;
      if (event.type === "pointerdown" && details.contains(event.target as Node)) return;
      details.open = false;
    };
    document.addEventListener("pointerdown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("pointerdown", close);
      document.removeEventListener("keydown", close);
    };
  }, []);

  return (
    <details className="column-chooser" ref={root}>
      <summary className="btn btn-ghost">{t("Columns")}</summary>
      <fieldset className="column-chooser-list">
        <legend className="visually-hidden">{t("Table columns")}</legend>
        {columns
          .filter((column) => !column.required)
          .map((column) => (
            <label key={column.key} className="check">
              <input type="checkbox" checked={isShown(column.key)} onChange={() => onToggle(column.key)} />
              {column.label}
            </label>
          ))}
        <Button type="button" variant="ghost" className="btn-small" onClick={onReset}>
          {t("Restore")}
        </Button>
      </fieldset>
    </details>
  );
}
