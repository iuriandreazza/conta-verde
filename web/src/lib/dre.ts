import type { DreLines } from "../api";
import { centsToApi, toCents } from "./entryInstallments";

/** The lines of the managerial income statement in the order they are read (ADR 0070). */
export interface DreRowSpec {
  key: keyof DreLines;
  label: string;
  /** A subtotal worked out by the server from the groups above it. */
  subtotal?: boolean;
  /** Below the line: shown apart, outside the result. */
  belowTheLine?: boolean;
}

export const DRE_ROWS: DreRowSpec[] = [
  { key: "gross_revenue", label: "Receita bruta" },
  { key: "deductions", label: "(−) Deduções" },
  { key: "net_revenue", label: "= Receita líquida", subtotal: true },
  { key: "costs", label: "(−) Custos" },
  { key: "gross_profit", label: "= Lucro bruto", subtotal: true },
  { key: "operating_expenses", label: "(−) Despesas operacionais" },
  { key: "operating_result", label: "= Resultado operacional", subtotal: true },
  { key: "financial_result", label: "(+/−) Resultado financeiro" },
  { key: "net_result", label: "= Resultado líquido", subtotal: true },
  { key: "investments", label: "Investimentos", belowTheLine: true },
  { key: "outside_result", label: "Empréstimos e capital", belowTheLine: true },
];

const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

/** `2026-10` as `out/2026`. */
export function monthColumnLabel(key: string): string {
  const [year, month] = key.split("-").map(Number);
  return `${MONTHS[month - 1]}/${year}`;
}

const cents = (amount: string) => toCents(amount.startsWith("-") ? amount.slice(1) : amount) ?? 0n;
const signed = (amount: string) => (amount.startsWith("-") ? -cents(amount) : cents(amount));

/** What a column spends on costs and operating expenses, as a positive amount for a chart; 0 when there is none. */
export function spending(lines: DreLines): string {
  const spent = -(signed(lines.costs) + signed(lines.operating_expenses));
  return spent > 0n ? centsToApi(spent) : "0";
}

/** What a column earns net of deductions, as a positive amount for a chart; 0 when it is not above 0. */
export function earning(lines: DreLines): string {
  const earned = signed(lines.net_revenue);
  return earned > 0n ? centsToApi(earned) : "0";
}
