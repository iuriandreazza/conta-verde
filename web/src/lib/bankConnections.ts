import type { BankConnection, BankSyncReport } from "../api";
import { formatDate } from "./entryInstallments";

export const STATUS_LABELS: Record<BankConnection["status"], string> = {
  connected: "Conectada",
  needs_attention: "Precisa de você",
  disconnected: "Desconectada",
};

export const STATUS_BADGES: Record<BankConnection["status"], string> = {
  connected: "badge badge-dot badge-ok",
  needs_attention: "badge badge-dot badge-warning",
  disconnected: "badge badge-muted",
};

/** What a pull did, in words: the banks' own movements that came in, the ones already known and the ones that could not be read. */
export function syncSummary(report: BankSyncReport): string {
  if (report.needs_attention) return "O banco pede uma ação sua: entre de novo na conexão pelo provedor e sincronize outra vez.";
  const parts = [`${report.created} nova${report.created === 1 ? "" : "s"}`, `${report.skipped} já conhecida${report.skipped === 1 ? "" : "s"}`];
  if (report.refused > 0) parts.push(`${report.refused} que não deu para ler`);
  return `Extrato sincronizado: ${parts.join(", ")}.`;
}

export const lastSync = (connection: BankConnection) => (connection.last_synced_on === null ? "Ainda não sincronizou" : formatDate(connection.last_synced_on));
