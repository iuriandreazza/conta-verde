import { useCallback } from "react";
import { api } from "../api";
import { describeInstallmentsText } from "../lib/installments";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = { name: "Nome", installments: "Parcelas", active: "Ativo" };

function display(field: string, value: string | null): string {
  if (value === null) return "—";
  if (field === "active") return value === "true" ? "sim" : "não";
  if (field === "installments") return describeInstallmentsText(value);
  return value;
}

export function PaymentTermHistory({ companyId, termId }: { companyId: string; termId: string }) {
  const load = useCallback(() => api.paymentTermHistory(companyId, termId), [companyId, termId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => FIELD_LABELS[field] ?? field}
      valueLabel={display}
      emptyMessage="Sem registros: este cadastro é anterior ao histórico. As próximas alterações aparecerão aqui."
    />
  );
}
