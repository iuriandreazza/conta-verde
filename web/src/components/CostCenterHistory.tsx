import { useCallback } from "react";
import { api } from "../api";
import { msg, useT, type Translate } from "../i18n";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = { name: msg("Name"), active: msg("Active") };

function display(field: string, value: string | null, t: Translate): string {
  if (value === null) return "—";
  if (field === "active") return value === "true" ? t("yes") : t("no");
  return value;
}

export function CostCenterHistory({ companyId, costCenterId }: { companyId: string; costCenterId: string }) {
  const t = useT();
  const load = useCallback(() => api.costCenterHistory(companyId, costCenterId), [companyId, costCenterId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)}
      valueLabel={(field, value) => display(field, value, t)}
      emptyMessage={t("No records: this record predates the history. The next changes will appear here.")}
    />
  );
}
