import type { DreLines } from "../api";
import { msg } from "../i18n";
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
  { key: "gross_revenue", label: msg("Gross revenue") },
  { key: "deductions", label: msg("(−) Deductions") },
  { key: "net_revenue", label: msg("= Net revenue"), subtotal: true },
  { key: "costs", label: msg("(−) Costs") },
  { key: "gross_profit", label: msg("= Gross profit"), subtotal: true },
  { key: "operating_expenses", label: msg("(−) Operating expenses") },
  { key: "operating_result", label: msg("= Operating result"), subtotal: true },
  { key: "financial_result", label: msg("(+/−) Financial result") },
  { key: "net_result", label: msg("= Net result"), subtotal: true },
  { key: "investments", label: msg("Investments"), belowTheLine: true },
  { key: "outside_result", label: msg("Loans and capital"), belowTheLine: true },
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
