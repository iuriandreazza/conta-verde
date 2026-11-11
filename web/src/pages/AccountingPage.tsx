import { useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type PeriodLock } from "../api";
import { AccountantDownloads } from "../components/AccountantDownloads";
import { Button } from "../components/Button";
import { CheckIcon } from "../components/icons/check";
import { Notice } from "../components/Notice";
import { PackageScheduleCard } from "../components/PackageScheduleCard";
import { PageHeader } from "../components/PageHeader";
import { PeriodLockHistory } from "../components/PeriodLockHistory";
import { Spinner } from "../components/Spinner";
import { MONTH, lastDayOfMonth, monthLabel, suggestedMonthToClose } from "../lib/closing";
import { formatDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { useCanWrite } from "../lib/useCanWrite";
import { HelpTitle } from "../components/HelpTitle";
import { useT } from "../i18n";

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function AccountingPage() {
  const t = useT();
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("accounting");
  // The server reads no clock, so the month to suggest is told with the day where the person is.
  const [today] = useState(() => todayIso());
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
    if (!MONTH.test(month)) {
      setError(t("Choose the month to close, written as 2026-10."));
      return;
    }
    const through = lastDayOfMonth(month);
    if (lock.locked_through !== null && through <= lock.locked_through) {
      setError(t("That month is already closed: the closing goes through {date}.", { date: formatDate(lock.locked_through) }));
      return;
    }
    void change(through, undefined, t("Period closed through {date}.", { date: formatDate(through) }));
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
      setError(t("Give the reason for reopening the period: it is kept in the history."));
      return;
    }
    if (month !== "" && !MONTH.test(month)) {
      setError(t("Write the month as 2026-10, or leave it empty to reopen everything."));
      return;
    }
    const through = month === "" ? null : lastDayOfMonth(month);
    if (through !== null && through >= lock.locked_through) {
      setError(t("To reopen, choose a month before the last closed month."));
      return;
    }
    // The reason is kept in the form when the server refuses, so it need not be typed again.
    void change(
      through,
      reason,
      through === null
        ? t("Period reopened: no month is closed.")
        : t("Period reopened: closed through {date}.", { date: formatDate(through) }),
    ).then((done) => done && target.reset());
  }

  const through = lock?.locked_through ?? null;
  return (
    <>
      <PageHeader
        title={t("Accountant")}
        description={t("Close the months already checked: nothing dated in them changes without reopening the period.")}
      />
      <Notice message={error} />
      <Notice message={saved} tone="success" />
      {lock === null && error === null ? (
        <p role="status" className="status">
          <Spinner /> {t("Loading…")}
        </p>
      ) : (
        lock !== null && (
          <>
            <section className="card lock-status">
              <HelpTitle level={2} title={t("Period closing")}>
                <p>
                  {t(
                    "Entries, settlements, transfers, opening balances, stock movements, completed sales and purchase receipts dated within the closed months cannot be created, changed or undone.",
                  )}
                </p>
              </HelpTitle>
              {through === null ? (
                <p className="lock-through">
                  <span className="badge badge-dot badge-muted">{t("Open")}</span>
                  <span>{t("No month is closed.")}</span>
                </p>
              ) : (
                <p className="lock-through">
                  <span className="badge badge-dot badge-ok">{t("Closed")}</span>
                  <span>{t("Closed through")}</span>
                  <strong className="lock-date">{formatDate(through)}</strong>
                  <span className="muted">{t("{month} inclusive", { month: monthLabel(through) })}</span>
                </p>
              )}
              <p className="muted">{t("Nothing dated in the closed months can be created, changed or undone.")}</p>
            </section>
            {canWrite && (
              <div className="lock-actions">
                <form onSubmit={close} className="card lock-form" key={`close-${through}`}>
                  <HelpTitle title={t("Close a month")}>
                    <p>{t("The period is closed through the last day of the chosen month, and the months before it with it.")}</p>
                  </HelpTitle>
                  <label>
                    {t("Close through the end of")}
                    <input type="month" name="month" defaultValue={suggestedMonthToClose(through, today)} />
                  </label>
                  <Button type="submit" icon={CheckIcon} pending={saving}>
                    {t("Close period")}
                  </Button>
                </form>
                {through !== null && (
                  <form onSubmit={reopen} className="card lock-form">
                    <HelpTitle title={t("Reopen")}>
                      <p>
                        {t("Choose through which month it stays closed, or leave it empty to reopen everything. The reason is kept in the history.")}
                      </p>
                    </HelpTitle>
                    <label>
                      {t("Keep closed through the end of")}
                      <input type="month" name="month" />
                    </label>
                    <label>
                      {t("Reason")}
                      <input name="reason" maxLength={200} />
                    </label>
                    <Button type="submit" variant="ghost" pending={saving}>
                      {t("Reopen period")}
                    </Button>
                  </form>
                )}
              </div>
            )}
            <AccountantDownloads companyId={companyId} lockedThrough={through} />
            <PackageScheduleCard companyId={companyId} />
            <section className="card">
              <h2>{t("History")}</h2>
              <PeriodLockHistory key={trail} companyId={companyId} />
            </section>
          </>
        )
      )}
    </>
  );
}
