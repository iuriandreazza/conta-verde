import { useCallback } from "react";
import { api } from "../api";
import { formatDate } from "../lib/entryInstallments";
import { msg, useT } from "../i18n";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = { locked_through: msg("Closed through"), reason: msg("Reason") };

/** Who closed and who reopened which months, newest first. */
export function PeriodLockHistory({ companyId }: { companyId: string }) {
  const t = useT();
  const load = useCallback(() => api.periodLockHistory(companyId), [companyId]);
  function display(field: string, value: string | null): string {
    if (value === null) return t("no month");
    return field === "locked_through" ? formatDate(value) : value;
  }
  return (
    <History
      load={load}
      fieldLabel={(field) => (field in FIELD_LABELS ? t(FIELD_LABELS[field]) : field)}
      valueLabel={display}
      emptyMessage={t("No month has been closed yet.")}
    />
  );
}
