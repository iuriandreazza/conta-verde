import { useCallback } from "react";
import { api } from "../api";
import { msg, useT, type Translate } from "../i18n";
import { formatDate } from "../lib/entryInstallments";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  legal_name: msg("Legal name"),
  cnpj: "CNPJ",
  state_registration: msg("State registration"),
  municipal_registration: msg("Municipal registration"),
  address: msg("Address"),
  regime: msg("Tax regime"),
  environment: msg("Environment"),
  nfe_series: msg("NF-e series"),
  nfce_series: msg("NFC-e series"),
  nfse_series: msg("NFS-e series"),
  certificate_valid_until: msg("Certificate expires on"),
  city_ibge_code: msg("Municipality IBGE code"),
  natures: msg("Operation natures"),
};

// The names of the tax regimes are the law's own words and are written the same in both languages.
const REGIMES: Record<string, string> = { simples_nacional: "Simples Nacional", lucro_presumido: "Lucro Presumido" };
const ENVIRONMENTS: Record<string, string> = { homologation: msg("Test environment"), production: msg("Production") };

// The trail keeps a nature as `code|name|kind|cfop in state|cfop interstate|active|rules`, the rules apart by `;` and
// each one `regime:ncm prefix:icms:pis:cofins:icms rate:pis rate:cofins rate`: written out so a changed rate shows. A
// service that has ISSQN terms adds an eighth part, `national code:municipal code:taxation:rate`.
function ruleLine(t: Translate, rule: string): string {
  const [regime, prefix, icms, pis, cofins, icmsRate, pisRate, cofinsRate] = rule.split(":");
  const rate = (label: string, code: string, value: string) =>
    value === "" ? `${label} ${code}` : t("{label} {code} at {rate}%", { label, code, rate: value });
  return `${REGIMES[regime] ?? regime}${prefix === "" ? "" : ` NCM ${prefix}*`}: ${rate("ICMS", icms, icmsRate)}, ${rate("PIS", pis, pisRate)}, ${rate("COFINS", cofins, cofinsRate)}`;
}

const TAXATIONS: Record<string, string> = {
  taxable: msg("taxable"),
  immune: msg("immune"),
  export: msg("export"),
  not_incident: msg("not incident"),
};

function serviceLine(t: Translate, terms: string): string {
  const [national, municipal, taxation, rate] = terms.split(":");
  const taxationText = taxation in TAXATIONS ? t(TAXATIONS[taxation]) : taxation;
  return [
    t("ISS: national code {code}", { code: national }),
    ...(municipal === "" ? [] : [t("municipal {code}", { code: municipal })]),
    `${taxationText}${rate === "" ? "" : ` ${t("at {rate}%", { rate })}`}`,
  ].join(", ");
}

function natureLine(t: Translate, row: string): string {
  const [code, name, kind, cfopIn, cfopOut, active, rules, terms] = row.split("|");
  const cfops = kind === "service" ? t("service") : `CFOP ${cfopIn}/${cfopOut}`;
  const detail = rules === "" ? "" : ` [${rules.split(";").map((rule) => ruleLine(t, rule)).join("; ")}]`;
  const iss = terms === undefined ? "" : ` [${serviceLine(t, terms)}]`;
  return `${name} (${code}), ${cfops}, ${active === "true" ? t("active") : t("inactive")}${detail}${iss}`;
}

export function FiscalSettingsHistory({ companyId }: { companyId: string }) {
  const t = useT();
  const load = useCallback(() => api.fiscalSettingsHistory(companyId), [companyId]);
  function display(field: string, value: string | null): string {
    if (value === null) return "—";
    if (field === "regime") return REGIMES[value] ?? value;
    if (field === "environment") return value in ENVIRONMENTS ? t(ENVIRONMENTS[value]) : value;
    if (field === "certificate_valid_until") return formatDate(value);
    if (field === "natures") {
      return value === "" ? t("none") : value.split("\n").map((row) => natureLine(t, row)).join(" · ");
    }
    return value;
  }
  return (
    <History
      load={load}
      fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)}
      valueLabel={display}
      emptyMessage={t("No records yet: the next changes will appear here.")}
    />
  );
}
