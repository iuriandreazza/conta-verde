import { useCallback } from "react";
import { api } from "../api";
import { msg, useT, type Translate } from "../i18n";
import { describeInstallmentsText } from "../lib/installments";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = { name: msg("Name"), installments: msg("Installments"), active: msg("Active") };

function display(field: string, value: string | null, t: Translate): string {
  if (value === null) return "—";
  if (field === "active") return value === "true" ? t("yes") : t("no");
  if (field === "installments") return describeInstallmentsText(value, t);
  return value;
}

export function PaymentTermHistory({ companyId, termId }: { companyId: string; termId: string }) {
  const t = useT();
  const load = useCallback(() => api.paymentTermHistory(companyId, termId), [companyId, termId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)}
      valueLabel={(field, value) => display(field, value, t)}
      emptyMessage={t("No records: this record predates the history. The next changes will appear here.")}
    />
  );
}
