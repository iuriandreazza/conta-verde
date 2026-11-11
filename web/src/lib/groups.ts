import type { CashFlowGroup, DreGroup } from "../api";
import { msg, type Translate } from "../i18n";

// The API sends stable keys; these are what people read.
export const DRE_GROUPS: { key: DreGroup; label: string }[] = [
  { key: "gross_revenue", label: msg("Gross revenue") },
  { key: "deductions", label: msg("Deductions") },
  { key: "costs", label: msg("Costs") },
  { key: "operating_expenses", label: msg("Operating expenses") },
  { key: "financial_result", label: msg("Financial result") },
  { key: "investments", label: msg("Investments") },
  { key: "outside_result", label: msg("Outside the result") },
  { key: "transfers", label: msg("Transfers between accounts") },
];

export const CASH_FLOW_GROUPS: { key: CashFlowGroup; label: string }[] = [
  { key: "operating", label: msg("Operating") },
  { key: "investing", label: msg("Investing") },
  { key: "financing", label: msg("Financing") },
  { key: "transfer", label: msg("Transfer") },
];

export function dreLabel(key: string, t: Translate): string {
  const label = DRE_GROUPS.find((group) => group.key === key)?.label;
  return label === undefined ? key : t(label);
}

export function cashFlowLabel(key: string, t: Translate): string {
  const label = CASH_FLOW_GROUPS.find((group) => group.key === key)?.label;
  return label === undefined ? key : t(label);
}
