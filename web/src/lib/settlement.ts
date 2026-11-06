import type { Entry, EntryInstallment } from "../api";
import { centsToApi, toCents } from "./entryInstallments";

export type InstallmentStatus = "open" | "partial" | "settled" | "overdue";

/** Today where the person is, as `YYYY-MM-DD`: the server does not know their zone (ADR 0030). */
export function todayIso(now: Date = new Date()): string {
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

/** What is left of an installment, in cents: its amount less what the settlements settled. */
export function balanceCents(installment: EntryInstallment): bigint {
  return (toCents(installment.amount) ?? 0n) - (toCents(installment.settled) ?? 0n);
}

/** What is left of an installment as the decimal the API takes. */
export function balance(installment: EntryInstallment): string {
  return centsToApi(balanceCents(installment));
}

/**
 * Settled when nothing is left, partial when something was settled and some is left, overdue when
 * something is left and it fell due before today, open otherwise. Overdue wins over partial: what is
 * late is what the person has to look at.
 */
export function installmentStatus(installment: EntryInstallment, today: string): InstallmentStatus {
  const left = balanceCents(installment);
  if (left <= 0n) return "settled";
  if (installment.due_date < today) return "overdue";
  return left < (toCents(installment.amount) ?? 0n) ? "partial" : "open";
}

const RANK: Record<InstallmentStatus, number> = { overdue: 3, partial: 2, open: 1, settled: 0 };

/** The entry's worst installment decides it: one overdue makes the entry overdue; all settled, settled. */
export function entryStatus(entry: Entry, today: string): InstallmentStatus {
  const statuses = entry.installments.map((installment) => installmentStatus(installment, today));
  if (statuses.every((status) => status === "settled")) return "settled";
  const pending = statuses.filter((status) => status !== "settled");
  // Some paid and the rest not yet due is partial, not open.
  const anySettled = statuses.some((status) => status === "settled" || status === "partial");
  const worst = pending.reduce((a, b) => (RANK[a] >= RANK[b] ? a : b));
  return worst === "open" && anySettled ? "partial" : worst;
}

export const STATUS_LABELS: Record<InstallmentStatus, string> = {
  open: "Em aberto",
  partial: "Parcial",
  settled: "Pago",
  overdue: "Vencido",
};
