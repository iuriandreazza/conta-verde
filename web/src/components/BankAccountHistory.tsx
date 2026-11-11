import { useCallback } from "react";
import { api } from "../api";
import { msg, useT, type Translate } from "../i18n";
import { bankAccountKindLabel } from "../lib/kinds";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  kind: msg("Type"),
  name: msg("Name"),
  bank_code: msg("Bank"),
  branch: msg("Branch"),
  number: msg("Account"),
  active: msg("Active account"),
};

function display(field: string, value: string | null, t: Translate): string {
  if (value === null) return "—";
  if (field === "active") return value === "true" ? t("yes") : t("no");
  if (field === "kind") return bankAccountKindLabel(value, t);
  return value;
}

export function BankAccountHistory({ companyId, accountId }: { companyId: string; accountId: string }) {
  const t = useT();
  const load = useCallback(() => api.bankAccountHistory(companyId, accountId), [companyId, accountId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)}
      valueLabel={(field, value) => display(field, value, t)}
      emptyMessage={t("No records: this record predates the history. The next changes will appear here.")}
    />
  );
}
