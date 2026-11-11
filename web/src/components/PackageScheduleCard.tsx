import { useEffect, useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import { api, errorMessage, type PackageSchedule } from "../api";
import { useT, type Translate } from "../i18n";
import { useCanWrite } from "../lib/useCanWrite";
import { Button } from "./Button";
import { HelpTitle } from "./HelpTitle";
import { Notice } from "./Notice";
import { Spinner } from "./Spinner";

// A day every month has (ADR 0072).
const DAYS = Array.from({ length: 28 }, (_, index) => index + 1);

/** Who the announcement reaches, said in words: with no accountant nothing is sent, and the page says so. */
function reach(t: Translate, accountants: number): string {
  if (accountants === 0) return t("No accountant was invited: nothing will be sent.");
  return accountants === 1 ? t("The notice goes to 1 accountant.") : t("The notice goes to {count} accountants.", { count: accountants });
}

/**
 * Whether the accountants are told, every month, that the previous month's package is ready, and from which day
 * (ADR 0072). The e-mail carries a link to this page and nothing else: the files are downloaded after signing in.
 */
export function PackageScheduleCard({ companyId }: { companyId: string }) {
  const t = useT();
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
      setSaved(stored.enabled ? t("Notice on: every month, from day {day}.", { day: stored.day }) : t("Notice off."));
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="card">
      <HelpTitle level={2} title={t("Monthly notice to accountants")}>
        <p>
          {t(
            "From the chosen day on, once a month, the company's accountants get an e-mail saying the previous month's package is ready. The e-mail carries a link to this page, where signing in with their own account is needed to download the files: it brings no company data or attachments.",
          )}
        </p>
      </HelpTitle>
      <Notice message={error} />
      <Notice message={saved} tone="success" />
      {schedule === null ? (
        error === null && (
          <p role="status" className="status">
            <Spinner /> {t("Loading…")}
          </p>
        )
      ) : (
        <>
          <p className="muted">
            {reach(t, schedule.accountants)}
            {schedule.accountants === 0 && canWrite && (
              <>
                {" "}
                <Link to={`/companies/${companyId}/members`}>{t("Invite an accountant")}</Link>
              </>
            )}
          </p>
          {canWrite ? (
            <form onSubmit={(event) => void save(event)} className="settle-form">
              <label className="inline-check">
                <input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} />
                {t("Notify every month")}
              </label>
              <label>
                {t("From day")}
                <select value={day} onChange={(event) => setDay(Number(event.target.value))}>
                  {DAYS.map((option) => (
                    <option key={option} value={option}>
                      {option}
                    </option>
                  ))}
                </select>
              </label>
              <Button type="submit" variant="ghost" pending={saving}>
                {t("Save notice")}
              </Button>
            </form>
          ) : (
            <p>{schedule.enabled ? t("On: every month, from day {day}.", { day: schedule.day }) : t("Off.")}</p>
          )}
        </>
      )}
    </section>
  );
}
