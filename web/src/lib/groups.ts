import type { CashFlowGroup, DreGroup } from "../api";

// The API sends stable keys; these are what people read.
export const DRE_GROUPS: { key: DreGroup; label: string }[] = [
  { key: "gross_revenue", label: "Receita bruta" },
  { key: "deductions", label: "Deduções" },
  { key: "costs", label: "Custos" },
  { key: "operating_expenses", label: "Despesas operacionais" },
  { key: "financial_result", label: "Resultado financeiro" },
  { key: "investments", label: "Investimentos" },
  { key: "outside_result", label: "Fora do resultado" },
  { key: "transfers", label: "Transferências entre contas" },
];

export const CASH_FLOW_GROUPS: { key: CashFlowGroup; label: string }[] = [
  { key: "operating", label: "Operacional" },
  { key: "investing", label: "Investimento" },
  { key: "financing", label: "Financiamento" },
  { key: "transfer", label: "Transferência" },
];

export function dreLabel(key: string): string {
  return DRE_GROUPS.find((group) => group.key === key)?.label ?? key;
}

export function cashFlowLabel(key: string): string {
  return CASH_FLOW_GROUPS.find((group) => group.key === key)?.label ?? key;
}
