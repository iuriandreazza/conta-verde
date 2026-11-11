import type { BankConnection, BankSyncReport } from "../api";
import { msg, type Translate } from "../i18n";
import { formatDate } from "./entryInstallments";

export const STATUS_LABELS: Record<BankConnection["status"], string> = {
  connected: msg("Connected"),
  needs_attention: msg("Needs you"),
  disconnected: msg("Disconnected"),
};

export const STATUS_BADGES: Record<BankConnection["status"], string> = {
  connected: "badge badge-dot badge-ok",
  needs_attention: "badge badge-dot badge-warning",
  disconnected: "badge badge-muted",
};

/** What a pull did, in words: the banks' own movements that came in, the ones already known and the ones that could not be read. */
export function syncSummary(report: BankSyncReport, t: Translate): string {
  if (report.needs_attention) return t("The bank asks for an action from you: sign in to the connection again through the provider and sync once more.");
  const parts = [
    report.created === 1 ? t("{count} new line", { count: report.created }) : t("{count} new lines", { count: report.created }),
    report.skipped === 1 ? t("{count} line already known", { count: report.skipped }) : t("{count} lines already known", { count: report.skipped }),
  ];
  if (report.refused > 0) parts.push(t("{count} that could not be read", { count: report.refused }));
  return t("Statement synced: {parts}.", { parts: parts.join(", ") });
}

export const lastSync = (connection: BankConnection, t: Translate) => (connection.last_synced_on === null ? t("Not synced yet") : formatDate(connection.last_synced_on));
