import { useCallback } from "react";
import { api } from "../api";
import { paymentMethodKindLabel } from "../lib/kinds";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = { kind: "Tipo", name: "Nome", active: "Ativa" };

function display(field: string, value: string | null): string {
  if (value === null) return "—";
  if (field === "active") return value === "true" ? "sim" : "não";
  if (field === "kind") return paymentMethodKindLabel(value);
  return value;
}

export function PaymentMethodHistory({ companyId, methodId }: { companyId: string; methodId: string }) {
  const load = useCallback(() => api.paymentMethodHistory(companyId, methodId), [companyId, methodId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => FIELD_LABELS[field] ?? field}
      valueLabel={display}
      emptyMessage="Sem registros: este cadastro é anterior ao histórico. As próximas alterações aparecerão aqui."
    />
  );
}
