import { useCallback } from "react";
import { api } from "../api";
import { msg, useT } from "../i18n";
import { History } from "./History";

// `revoked` shares its key with the status of a key, which is feminine in Portuguese and so carries the empty `{feminine}` part.
const FIELD_LABELS: Record<string, string> = {
  name: msg("Name"),
  scopes: msg("Scopes"),
  expires_at: msg("Expires on"),
  revoked: msg("Revoked{feminine}"),
};

export function ApiKeyHistory({ companyId, keyId }: { companyId: string; keyId: string }) {
  const t = useT();
  const load = useCallback(() => api.apiKeyHistory(companyId, keyId), [companyId, keyId]);

  function display(field: string, value: string | null): string {
    if (value === null) return "—";
    if (field === "revoked") return value === "true" ? t("yes") : t("no");
    if (field === "expires_at") return new Date(Number(value)).toLocaleString("pt-BR");
    return value;
  }

  return (
    <History
      load={load}
      fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field], { feminine: "" }) : field)}
      valueLabel={display}
      emptyMessage={t("No records.")}
    />
  );
}
