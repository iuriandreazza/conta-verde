import { useCallback } from "react";
import { api } from "../api";
import { msg, useT, type Translate } from "../i18n";
import { cashFlowLabel, dreLabel } from "../lib/groups";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  name: msg("Name"),
  parent: msg("Position"),
  dre_group: msg("DRE group"),
  cash_flow_group: msg("Cash flow group"),
  active: msg("Active category"),
};

function display(field: string, value: string | null, t: Translate): string {
  if (field === "parent") return value === null ? t("top-level category") : t("subcategory");
  if (value === null) return "—";
  if (field === "active") return value === "true" ? t("yes") : t("no");
  if (field === "dre_group") return dreLabel(value, t);
  if (field === "cash_flow_group") return cashFlowLabel(value, t);
  return value;
}

export function CategoryHistory({ companyId, categoryId }: { companyId: string; categoryId: string }) {
  const t = useT();
  const load = useCallback(() => api.categoryHistory(companyId, categoryId), [companyId, categoryId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)}
      valueLabel={(field, value) => display(field, value, t)}
      emptyMessage={t("No records: this record predates the history. The next changes will appear here.")}
    />
  );
}
