import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api, errorMessage, type ExportFile, type ExportFormat } from "../api";
import { saveFile } from "../lib/csv";
import { MONTH, lastDayOfMonth, monthToExport } from "../lib/closing";
import { todayIso } from "../lib/settlement";
import { msg, useT } from "../i18n";
import { Button } from "./Button";
import { HelpTitle } from "./HelpTitle";
import { Notice } from "./Notice";

const FILES: { file: ExportFile; label: string }[] = [
  { file: "entries", label: msg("Entries") },
  { file: "settlements", label: msg("Settlements") },
  { file: "transfers", label: msg("Transfers") },
];

/**
 * The accountant's files for a month (ADR 0071): entries by competence date, settlements by the day they were paid,
 * transfers, or all of them with the attached files in one zip. It says what the package does not hold.
 */
export function AccountantDownloads({ companyId, lockedThrough }: { companyId: string; lockedThrough: string | null }) {
  const t = useT();
  // The e-mail that says a package is ready links here with the month in the address (ADR 0072).
  const [params] = useSearchParams();
  const linked = params.get("month") ?? "";
  const [month, setMonth] = useState(() => (MONTH.test(linked) ? linked : monthToExport(lockedThrough, todayIso())));
  const [format, setFormat] = useState<ExportFormat>("csv");
  // The month follows the lock when a month is closed here, but the first one shown is the one the e-mail linked to.
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    setMonth(monthToExport(lockedThrough, todayIso()));
  }, [lockedThrough]);
  const [pending, setPending] = useState<ExportFile | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function download(file: ExportFile) {
    if (!MONTH.test(month)) {
      setError(t("Choose the month, written as 2026-10."));
      return;
    }
    setPending(file);
    setError(null);
    try {
      const saved = await api.exportFile(companyId, file, `${month}-01`, lastDayOfMonth(month), format);
      saveFile(saved.filename, saved.blob);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setPending(null);
    }
  }

  return (
    <section className="card">
      <HelpTitle level={2} title={t("Files for the accountant")}>
        <p>
          {t(
            "Entries by competence date (one row per installment), settlements by the day they were made and the month's transfers. The package gathers everything and carries the files attached to the entries. What was cancelled or reversed is left out.",
          )}
        </p>
      </HelpTitle>
      <p className="muted">
        {t(
          "The system does not keep the XML of purchase invoices: of what comes in, the package carries only what was attached to the entries. Of the invoices issued in production in the period, it carries the list (fiscal.csv) and their files: XML, DANFE, cancellations and correction letters.",
        )}
      </p>
      <Notice message={error} />
      <div className="settle-form">
        <label>
          {t("Month")}
          <input type="month" value={month} onChange={(event) => setMonth(event.target.value)} />
        </label>
        <label>
          {t("Format")}
          <select value={format} onChange={(event) => setFormat(event.target.value as ExportFormat)}>
            <option value="csv">{t("CSV (opens in Excel)")}</option>
            <option value="xlsx">{t("Spreadsheet (.xlsx)")}</option>
          </select>
        </label>
      </div>
      <div className="button-row">
        {FILES.map(({ file, label }) => (
          <Button
            key={file}
            type="button"
            variant="ghost"
            pending={pending === file}
            disabled={pending !== null}
            onClick={() => void download(file)}
          >
            {t(label)}
          </Button>
        ))}
        <Button type="button" pending={pending === "package"} disabled={pending !== null} onClick={() => void download("package")}>
          {t("Complete package (.zip)")}
        </Button>
      </div>
    </section>
  );
}
