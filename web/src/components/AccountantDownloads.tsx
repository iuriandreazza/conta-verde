import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { api, errorMessage, type ExportFile, type ExportFormat } from "../api";
import { saveFile } from "../lib/csv";
import { MONTH, lastDayOfMonth, monthToExport } from "../lib/closing";
import { todayIso } from "../lib/settlement";
import { Button } from "./Button";
import { HelpTitle } from "./HelpTitle";
import { Notice } from "./Notice";

const FILES: { file: ExportFile; label: string }[] = [
  { file: "entries", label: "Lançamentos" },
  { file: "settlements", label: "Baixas" },
  { file: "transfers", label: "Transferências" },
];

/**
 * The accountant's files for a month (ADR 0071): entries by competence date, settlements by the day they were paid,
 * transfers, or all of them with the attached files in one zip. It says what the package does not hold.
 */
export function AccountantDownloads({ companyId, lockedThrough }: { companyId: string; lockedThrough: string | null }) {
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
      setError("Escolha o mês, escrito como 2026-10.");
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
      <HelpTitle level={2} title="Arquivos para o contador">
        <p>
          Lançamentos por data de competência (uma linha por parcela), baixas pela data em que foram feitas e
          transferências do mês. O pacote junta tudo e leva os arquivos anexados aos lançamentos. O que foi cancelado ou
          estornado fica de fora.
        </p>
      </HelpTitle>
      <p className="muted">
        O sistema não guarda o XML das notas de compra e não emite notas fiscais: o pacote leva só o que foi anexado aos
        lançamentos.
      </p>
      <Notice message={error} />
      <div className="settle-form">
        <label>
          Mês
          <input type="month" value={month} onChange={(event) => setMonth(event.target.value)} />
        </label>
        <label>
          Formato
          <select value={format} onChange={(event) => setFormat(event.target.value as ExportFormat)}>
            <option value="csv">CSV (abre no Excel)</option>
            <option value="xlsx">Planilha (.xlsx)</option>
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
            {label}
          </Button>
        ))}
        <Button type="button" pending={pending === "package"} disabled={pending !== null} onClick={() => void download("package")}>
          Pacote completo (.zip)
        </Button>
      </div>
    </section>
  );
}
