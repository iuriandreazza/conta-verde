import { useCallback } from "react";
import { api } from "../api";
import { bankAccountKindLabel } from "../lib/kinds";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  kind: "Tipo",
  name: "Nome",
  bank_code: "Banco",
  branch: "Agência",
  number: "Conta",
  active: "Ativa",
};

function display(field: string, value: string | null): string {
  if (value === null) return "—";
  if (field === "active") return value === "true" ? "sim" : "não";
  if (field === "kind") return bankAccountKindLabel(value);
  return value;
}

export function BankAccountHistory({ companyId, accountId }: { companyId: string; accountId: string }) {
  const load = useCallback(() => api.bankAccountHistory(companyId, accountId), [companyId, accountId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => FIELD_LABELS[field] ?? field}
      valueLabel={display}
      emptyMessage="Sem registros: este cadastro é anterior ao histórico. As próximas alterações aparecerão aqui."
    />
  );
}
