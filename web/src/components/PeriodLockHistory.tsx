import { useCallback } from "react";
import { api } from "../api";
import { formatDate } from "../lib/entryInstallments";
import { History } from "./History";

const FIELD_LABELS: Record<string, string> = { locked_through: "Fechado até", reason: "Motivo" };

function display(field: string, value: string | null): string {
  if (value === null) return "nenhum mês";
  return field === "locked_through" ? formatDate(value) : value;
}

/** Who closed and who reopened which months, newest first. */
export function PeriodLockHistory({ companyId }: { companyId: string }) {
  const load = useCallback(() => api.periodLockHistory(companyId), [companyId]);
  return (
    <History
      load={load}
      fieldLabel={(field) => FIELD_LABELS[field] ?? field}
      valueLabel={display}
      emptyMessage="Nenhum mês foi fechado ainda."
    />
  );
}
