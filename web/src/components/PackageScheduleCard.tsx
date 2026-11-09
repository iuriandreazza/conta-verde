import { useEffect, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import { api, errorMessage, type PackageSchedule } from "../api";
import { useCanWrite } from "../lib/useCanWrite";
import { Button } from "./Button";
import { HelpTitle } from "./HelpTitle";
import { Notice } from "./Notice";
import { Spinner } from "./Spinner";

// A day every month has (ADR 0072).
const DAYS = Array.from({ length: 28 }, (_, index) => index + 1);

/** Who the announcement reaches, said in words: with no accountant nothing is sent, and the page says so. */
function reach(accountants: number): string {
  if (accountants === 0) return "Nenhum contador foi convidado: nada será enviado.";
  return accountants === 1 ? "O aviso vai para 1 contador." : `O aviso vai para ${accountants} contadores.`;
}

/**
 * Whether the accountants are told, every month, that the previous month's package is ready, and from which day
 * (ADR 0072). The e-mail carries a link to this page and nothing else: the files are downloaded after signing in.
 */
export function PackageScheduleCard({ companyId }: { companyId: string }) {
  const canWrite = useCanWrite("accounting");
  const [schedule, setSchedule] = useState<PackageSchedule | null>(null);
  const [enabled, setEnabled] = useState(false);
  const [day, setDay] = useState(5);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let current = true;
    api
      .getPackageSchedule(companyId)
      .then((stored) => {
        if (!current) return;
        setSchedule(stored);
        setEnabled(stored.enabled);
        setDay(stored.day);
      })
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) return;
    setSaving(true);
    setError(null);
    setSaved(null);
    try {
      const stored = await api.replacePackageSchedule(companyId, { enabled, day });
      setSchedule(stored);
      setSaved(stored.enabled ? `Aviso ligado: todo mês, a partir do dia ${stored.day}.` : "Aviso desligado.");
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="card">
      <HelpTitle level={2} title="Aviso mensal aos contadores">
        <p>
          Do dia escolhido em diante, uma vez por mês, os contadores da empresa recebem um e-mail avisando que o pacote do
          mês anterior está pronto. O e-mail leva um link para esta página, onde é preciso entrar com a própria conta para
          baixar os arquivos: ele não traz dados da empresa nem anexos.
        </p>
      </HelpTitle>
      <Notice message={error} />
      <Notice message={saved} tone="success" />
      {schedule === null ? (
        error === null && (
          <p role="status" className="status">
            <Spinner /> Carregando…
          </p>
        )
      ) : (
        <>
          <p className="muted">
            {reach(schedule.accountants)}
            {schedule.accountants === 0 && canWrite && (
              <>
                {" "}
                <Link to={`/companies/${companyId}/members`}>Convidar um contador</Link>
              </>
            )}
          </p>
          {canWrite ? (
            <form onSubmit={(event) => void save(event)} className="settle-form">
              <label className="inline-check">
                <input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} />
                Avisar todo mês
              </label>
              <label>
                A partir do dia
                <select value={day} onChange={(event) => setDay(Number(event.target.value))}>
                  {DAYS.map((option) => (
                    <option key={option} value={option}>
                      {option}
                    </option>
                  ))}
                </select>
              </label>
              <Button type="submit" variant="ghost" pending={saving}>
                Salvar aviso
              </Button>
            </form>
          ) : (
            <p>{schedule.enabled ? `Ligado: todo mês, a partir do dia ${schedule.day}.` : "Desligado."}</p>
          )}
        </>
      )}
    </section>
  );
}
