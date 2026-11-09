import { useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type PeriodLock } from "../api";
import { Button } from "../components/Button";
import { CheckIcon } from "../components/icons/check";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { PeriodLockHistory } from "../components/PeriodLockHistory";
import { Spinner } from "../components/Spinner";
import { lastDayOfMonth, monthLabel } from "../lib/closing";
import { formatDate } from "../lib/entryInstallments";
import { useCanWrite } from "../lib/useCanWrite";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function AccountingPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("accounting");
  const [lock, setLock] = useState<PeriodLock | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // Bumped after a change so the trail below reads again.
  const [trail, setTrail] = useState(0);

  useEffect(() => {
    let current = true;
    api
      .getPeriodLock(companyId)
      .then((stored) => current && setLock(stored))
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  async function change(through: string | null, reason: string | undefined, done: string): Promise<boolean> {
    setSaving(true);
    setError(null);
    setSaved(null);
    try {
      setLock(await api.replacePeriodLock(companyId, { locked_through: through, reason }));
      setTrail((count) => count + 1);
      setSaved(done);
      return true;
    } catch (failure) {
      setError(errorMessage(failure));
      return false;
    } finally {
      setSaving(false);
    }
  }

  function close(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving || lock === null) {
      return;
    }
    const month = String(new FormData(event.currentTarget).get("month") ?? "");
    if (month === "") {
      setError("Escolha o mês que será fechado.");
      return;
    }
    const through = lastDayOfMonth(month);
    if (lock.locked_through !== null && through <= lock.locked_through) {
      setError(`Esse mês já está fechado: o fechamento vai até ${formatDate(lock.locked_through)}.`);
      return;
    }
    void change(through, undefined, `Período fechado até ${formatDate(through)}.`);
  }

  function reopen(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving || lock === null || lock.locked_through === null) {
      return;
    }
    const target = event.currentTarget;
    const form = new FormData(target);
    const month = String(form.get("month") ?? "");
    const reason = String(form.get("reason") ?? "").trim();
    if (reason === "") {
      setError("Diga o motivo de reabrir o período: ele fica no histórico.");
      return;
    }
    const through = month === "" ? null : lastDayOfMonth(month);
    if (through !== null && through >= lock.locked_through) {
      setError("Para reabrir, escolha um mês anterior ao último mês fechado.");
      return;
    }
    // The reason is kept in the form when the server refuses, so it need not be typed again.
    void change(
      through,
      reason,
      through === null ? "Período reaberto: nenhum mês está fechado." : `Período reaberto: fechado até ${formatDate(through)}.`,
    ).then((done) => done && target.reset());
  }

  const through = lock?.locked_through ?? null;
  return (
    <>
      <PageHeader
        title="Contador"
        description="Feche os meses já conferidos: nada datado neles muda sem reabrir o período."
      />
      <Notice message={error} />
      <Notice message={saved} tone="success" />
      {lock === null && error === null ? (
        <p role="status" className="status">
          <Spinner /> Carregando…
        </p>
      ) : (
        lock !== null && (
          <section className="card">
            <h2>Fechamento de período</h2>
            <p>
              {through === null ? (
                "Nenhum mês está fechado."
              ) : (
                <>
                  Fechado até <strong>{formatDate(through)}</strong> ({monthLabel(through)} inclusive).
                </>
              )}
            </p>
            <p className="muted">
              Lançamentos, baixas, transferências, saldos iniciais, movimentos de estoque, vendas concluídas e
              recebimentos de compra com data dentro dos meses fechados não podem ser criados, alterados nem
              desfeitos.
            </p>
            {canWrite && (
              <>
                <form onSubmit={close} className="settle-form">
                  <label>
                    Fechar até o fim de
                    <input type="month" name="month" />
                  </label>
                  <Button type="submit" icon={CheckIcon} pending={saving}>
                    Fechar período
                  </Button>
                </form>
                {through !== null && (
                  <form onSubmit={reopen} className="settle-form">
                    <h3>Reabrir</h3>
                    <label>
                      Manter fechado até o fim de (vazio reabre tudo)
                      <input type="month" name="month" />
                    </label>
                    <label>
                      Motivo
                      <input name="reason" maxLength={200} />
                    </label>
                    <Button type="submit" variant="ghost" pending={saving}>
                      Reabrir período
                    </Button>
                  </form>
                )}
              </>
            )}
            <h3>Histórico</h3>
            <PeriodLockHistory key={trail} companyId={companyId} />
          </section>
        )
      )}
    </>
  );
}
