import { useCallback } from "react";
import { api } from "../api";
import { STATUS_LABELS } from "../lib/bankConnections";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  provider: "Provedor",
  bank_account_id: "Conta bancária",
  item_id: "Conexão",
  account_id: "Conta no provedor",
  status: "Estado",
};

/** `accountName` turns the id of a bank account of the company into its name, which is what a person recognizes. */
export function BankConnectionHistory({ companyId, id, accountName }: { companyId: string; id: string; accountName: (id: string) => string }) {
  const load = useCallback(() => api.bankConnectionHistory(companyId, id), [companyId, id]);
  const display = (field: string, value: string | null) => {
    if (value === null) return "—";
    if (field === "status") return STATUS_LABELS[value as keyof typeof STATUS_LABELS] ?? value;
    return field === "bank_account_id" ? accountName(value) : value;
  };
  return <History load={load} fieldLabel={(field) => FIELD_LABELS[field] ?? field} valueLabel={display} emptyMessage="Sem registros." />;
}
