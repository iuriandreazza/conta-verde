import { useCallback } from "react";
import { api } from "../api";
import { msg, useT } from "../i18n";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = {
  kind: msg("Type"),
  name: msg("Name"),
  active: msg("Turned on"),
  api_key_id: msg("Key"),
  webhook_subscription_id: msg("Webhook"),
};

export function ConnectorHistory({ companyId, id }: { companyId: string; id: string }) {
  const t = useT();
  const load = useCallback(() => api.connectorHistory(companyId, id), [companyId, id]);
  const display = (field: string, value: string | null) => {
    if (value === null) return "—";
    return field === "active" ? (value === "true" ? t("yes") : t("no")) : value;
  };
  return <History load={load} fieldLabel={(field) => t(FIELD_LABELS[field] ?? field)} valueLabel={display} emptyMessage={t("No records.")} />;
}
