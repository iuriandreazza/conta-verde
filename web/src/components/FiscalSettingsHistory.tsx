import { useCallback } from "react";
import { api } from "../api";
import { formatDate } from "../lib/entryInstallments";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  legal_name: "Razão social",
  cnpj: "CNPJ",
  state_registration: "Inscrição estadual",
  municipal_registration: "Inscrição municipal",
  address: "Endereço",
  regime: "Regime tributário",
  environment: "Ambiente",
  nfe_series: "Série da NF-e",
  nfce_series: "Série da NFC-e",
  nfse_series: "Série da NFS-e",
  certificate_valid_until: "Certificado vence em",
  natures: "Naturezas de operação",
};

const REGIMES: Record<string, string> = { simples_nacional: "Simples Nacional", lucro_presumido: "Lucro Presumido" };
const ENVIRONMENTS: Record<string, string> = { homologation: "Homologação", production: "Produção" };

function display(field: string, value: string | null): string {
  if (value === null) return "—";
  if (field === "regime") return REGIMES[value] ?? value;
  if (field === "environment") return ENVIRONMENTS[value] ?? value;
  if (field === "certificate_valid_until") return formatDate(value);
  if (field === "natures") {
    const count = value === "" ? 0 : value.split("\n").length;
    return count === 1 ? "1 natureza" : `${count} naturezas`;
  }
  return value;
}

export function FiscalSettingsHistory({ companyId }: { companyId: string }) {
  const load = useCallback(() => api.fiscalSettingsHistory(companyId), [companyId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => FIELD_LABELS[field] ?? field}
      valueLabel={display}
      emptyMessage="Ainda não há registros: as próximas alterações aparecerão aqui."
    />
  );
}
