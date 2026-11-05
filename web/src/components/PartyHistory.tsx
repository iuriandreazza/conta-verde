import { useCallback } from "react";
import { api } from "../api";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  name: "Nome",
  document: "Documento",
  email: "E-mail",
  is_customer: "Cliente",
  is_supplier: "Fornecedor",
  document_check: "Verificação do CNPJ",
  registry_legal_name: "Razão social (Receita)",
  registry_situation: "Situação (Receita)",
};

const CHECK_LABELS: Record<string, string> = {
  verified: "Verificado",
  not_found: "Não encontrado",
  unverified: "Não verificado",
};

function label(field: string): string {
  return FIELD_LABELS[field] ?? field;
}

function display(field: string, value: string | null): string {
  if (value === null) return "—";
  if (field === "is_customer" || field === "is_supplier") return value === "true" ? "sim" : "não";
  if (field === "document_check") return CHECK_LABELS[value] ?? value;
  return value;
}

export function PartyHistory({ companyId, partyId }: { companyId: string; partyId: string }) {
  const load = useCallback(() => api.partyHistory(companyId, partyId), [companyId, partyId]);
  return (
    <History
      load={load}
      fieldLabel={label}
      valueLabel={display}
      // Parties created before the audit trail existed have no entries until they change.
      emptyMessage="Sem registros: este cadastro é anterior ao histórico. As próximas alterações aparecerão aqui."
    />
  );
}
