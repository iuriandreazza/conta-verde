import { useCallback } from "react";
import { api } from "../api";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  name: "Nome",
  scopes: "Escopos",
  expires_at: "Expira em",
  revoked: "Revogada",
};

function display(field: string, value: string | null): string {
  if (value === null) return "—";
  if (field === "revoked") return value === "true" ? "sim" : "não";
  if (field === "expires_at") return new Date(Number(value)).toLocaleString("pt-BR");
  return value;
}

export function ApiKeyHistory({ companyId, keyId }: { companyId: string; keyId: string }) {
  const load = useCallback(() => api.apiKeyHistory(companyId, keyId), [companyId, keyId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => FIELD_LABELS[field] ?? field}
      valueLabel={display}
      emptyMessage="Sem registros."
    />
  );
}
