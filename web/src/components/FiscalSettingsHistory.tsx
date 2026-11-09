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

// The trail keeps a nature as `code|name|kind|cfop in state|cfop interstate|active|rules`, the rules apart by `;` and
// each one `regime:ncm prefix:icms:pis:cofins:icms rate:pis rate:cofins rate`: written out so a changed rate shows.
function ruleLine(rule: string): string {
  const [regime, prefix, icms, pis, cofins, icmsRate, pisRate, cofinsRate] = rule.split(":");
  const rate = (label: string, code: string, value: string) => `${label} ${code}${value === "" ? "" : ` a ${value}%`}`;
  return `${REGIMES[regime] ?? regime}${prefix === "" ? "" : ` NCM ${prefix}*`}: ${rate("ICMS", icms, icmsRate)}, ${rate("PIS", pis, pisRate)}, ${rate("COFINS", cofins, cofinsRate)}`;
}

function natureLine(row: string): string {
  const [code, name, kind, cfopIn, cfopOut, active, rules] = row.split("|");
  const cfops = kind === "service" ? "serviço" : `CFOP ${cfopIn}/${cfopOut}`;
  const detail = rules === "" ? "" : ` [${rules.split(";").map(ruleLine).join("; ")}]`;
  return `${name} (${code}), ${cfops}, ${active === "true" ? "ativa" : "inativa"}${detail}`;
}

function display(field: string, value: string | null): string {
  if (value === null) return "—";
  if (field === "regime") return REGIMES[value] ?? value;
  if (field === "environment") return ENVIRONMENTS[value] ?? value;
  if (field === "certificate_valid_until") return formatDate(value);
  if (field === "natures") {
    return value === "" ? "nenhuma" : value.split("\n").map(natureLine).join(" · ");
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
