import { useCallback } from "react";
import { api } from "../api";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = { name: "Nome", active: "Ativo" };

function display(field: string, value: string | null): string {
  if (value === null) return "—";
  if (field === "active") return value === "true" ? "sim" : "não";
  return value;
}

export function CostCenterHistory({ companyId, costCenterId }: { companyId: string; costCenterId: string }) {
  const load = useCallback(() => api.costCenterHistory(companyId, costCenterId), [companyId, costCenterId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => FIELD_LABELS[field] ?? field}
      valueLabel={display}
      emptyMessage="Sem registros: este cadastro é anterior ao histórico. As próximas alterações aparecerão aqui."
    />
  );
}
