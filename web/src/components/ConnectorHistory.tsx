import { useCallback } from "react";
import { api } from "../api";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  kind: "Tipo",
  name: "Nome",
  active: "Ligado",
  api_key_id: "Chave",
  webhook_subscription_id: "Webhook",
};

const display = (field: string, value: string | null) => {
  if (value === null) return "—";
  return field === "active" ? (value === "true" ? "sim" : "não") : value;
};

export function ConnectorHistory({ companyId, id }: { companyId: string; id: string }) {
  const load = useCallback(() => api.connectorHistory(companyId, id), [companyId, id]);
  return <History load={load} fieldLabel={(field) => FIELD_LABELS[field] ?? field} valueLabel={display} emptyMessage="Sem registros." />;
}
