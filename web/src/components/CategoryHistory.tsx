import { useCallback } from "react";
import { api } from "../api";
import { cashFlowLabel, dreLabel } from "../lib/groups";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  name: "Nome",
  parent: "Posição",
  dre_group: "Grupo no DRE",
  cash_flow_group: "Grupo no fluxo de caixa",
  active: "Ativa",
};

function display(field: string, value: string | null): string {
  if (field === "parent") return value === null ? "categoria principal" : "subcategoria";
  if (value === null) return "—";
  if (field === "active") return value === "true" ? "sim" : "não";
  if (field === "dre_group") return dreLabel(value);
  if (field === "cash_flow_group") return cashFlowLabel(value);
  return value;
}

export function CategoryHistory({ companyId, categoryId }: { companyId: string; categoryId: string }) {
  const load = useCallback(() => api.categoryHistory(companyId, categoryId), [companyId, categoryId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => FIELD_LABELS[field] ?? field}
      valueLabel={display}
      emptyMessage="Sem registros: este cadastro é anterior ao histórico. As próximas alterações aparecerão aqui."
    />
  );
}
