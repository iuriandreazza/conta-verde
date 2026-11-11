import { useCallback } from "react";
import { api } from "../api";
import { msg, useT } from "../i18n";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = { url: msg("Address"), events: msg("Events"), active: msg("Active"), secret: msg("Secret") };

export function WebhookHistory({ companyId, id }: { companyId: string; id: string }) {
  const t = useT();
  const load = useCallback(() => api.webhookHistory(companyId, id), [companyId, id]);

  function display(field: string, value: string | null): string {
    if (value === null) return "—";
    if (field === "active") return value === "true" ? t("yes") : t("no");
    if (field === "events" && value === "all") return t("all");
    if (field === "secret") return value === "set" ? t("set") : t("replaced");
    return value;
  }

  return <History load={load} fieldLabel={(field) => t(FIELD_LABELS[field] ?? field)} valueLabel={display} emptyMessage={t("No records.")} />;
}
