import { useCallback } from "react";
import { api } from "../api";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = { url: "Endereço", events: "Eventos", active: "Ativo", secret: "Segredo" };

function display(field: string, value: string | null): string {
  if (value === null) return "—";
  if (field === "active") return value === "true" ? "sim" : "não";
  if (field === "events" && value === "all") return "todos";
  if (field === "secret") return value === "set" ? "definido" : "trocado";
  return value;
}

export function WebhookHistory({ companyId, id }: { companyId: string; id: string }) {
  const load = useCallback(() => api.webhookHistory(companyId, id), [companyId, id]);
  return <History load={load} fieldLabel={(field) => FIELD_LABELS[field] ?? field} valueLabel={display} emptyMessage="Sem registros." />;
}
