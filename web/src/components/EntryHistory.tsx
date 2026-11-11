import { useCallback } from "react";
import { api } from "../api";
import { formatMoney } from "../lib/decimal";
import { formatDate } from "../lib/entryInstallments";
import { msg, useT, type Translate } from "../i18n";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  kind: msg("Type"),
  description: msg("Description"),
  party_id: msg("Customer or supplier"),
  category_id: msg("Category"),
  cost_center_id: msg("Cost center"),
  competence_date: msg("Accrual date"),
  installments: msg("Installments"),
  voided: msg("Voided"),
};

/** `1:2026-12-10:100.5,2:2027-01-10:100.5` as `10/12/2026 R$ 100,50 · 10/01/2027 R$ 100,50`. */
function describeInstallments(text: string): string {
  return text
    .split(",")
    .map((part) => {
      const [, due, amount] = part.split(":");
      return `${formatDate(due)} ${formatMoney(amount)}`;
    })
    .join(" · ");
}

const REFERENCES = ["party_id", "category_id", "cost_center_id"];

function display(
  field: string,
  value: string | null,
  nameOf: (id: string) => string | undefined,
  t: Translate,
): string {
  if (value === null) return "—";
  // A reference is stored as an id; people read the name, when the page knows it.
  if (REFERENCES.includes(field)) return nameOf(value) ?? value;
  if (field === "kind") return value === "receivable" ? t("Receivable") : t("Payable");
  if (field === "competence_date") return formatDate(value);
  if (field === "installments") return describeInstallments(value);
  if (field === "voided") return value === "true" ? t("Yes") : t("No");
  return value;
}

interface EntryHistoryProps {
  companyId: string;
  entryId: string;
  /** The name of a category, cost center or party, by id. */
  nameOf: (id: string) => string | undefined;
}

export function EntryHistory({ companyId, entryId, nameOf }: EntryHistoryProps) {
  const t = useT();
  const load = useCallback(() => api.entryHistory(companyId, entryId), [companyId, entryId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)}
      valueLabel={(field, value) => display(field, value, nameOf, t)}
      emptyMessage={t("No records: this entry predates the history. The next changes will appear here.")}
    />
  );
}
