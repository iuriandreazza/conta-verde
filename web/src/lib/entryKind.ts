import type { EntryKind } from "../api";

/** The kind a `?tipo=` asks for; anything else, or nothing, is every kind. */
export function entryKindOf(search: string): EntryKind | "" {
  const tipo = new URLSearchParams(search).get("tipo");
  return tipo === "receivable" || tipo === "payable" ? tipo : "";
}
