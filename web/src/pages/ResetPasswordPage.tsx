import { useState, type FormEvent } from "react";
import { Link, useParams } from "react-router-dom";
import { ApiError, api, errorMessage } from "../api";
import { useAuth } from "../auth";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { PasswordField } from "../components/PasswordField";
import { useT } from "../i18n";
import { usePageTitle } from "../lib/usePageTitle";

/**
 * Where the link of the e-mail leads: a new password for the account. The link works once and every session of the
 * account ends, so the person signs in again after it.
 */
export function ResetPasswordPage() {
  const t = useT();
  const { token = "" } = useParams();
  usePageTitle(t("New password"));
  const { refresh } = useAuth();
  const [done, setDone] = useState(false);
  const [error, setError] = useState<{ message: string; gone: boolean } | null>(null);
  const [submitting, setSubmitting] = useState(false);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const password = String(new FormData(event.currentTarget).get("password") ?? "");
    setSubmitting(true);
    setError(null);
    try {
      await api.resetPassword(token, password);
      setDone(true);
      // Every session of the account ended: a browser still signed in finds out now (its 401 signs it out); a
      // browser that was not signed in has nothing to find out.
      refresh().catch(() => undefined);
    } catch (failure) {
      setError({ message: errorMessage(failure), gone: failure instanceof ApiError && failure.status === 410 });
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <section className="card narrow">
      <h1>{t("New password")}</h1>
      {done ? (
        <>
          <Notice tone="success" message={t("Password changed. Sign in with the new password.")} />
          <p>
            <Link to="/login">{t("Sign in")}</Link>
          </p>
        </>
      ) : (
        <>
          <p className="page-lead">{t("Choose your account's new password.")}</p>
          <form onSubmit={handleSubmit}>
            <PasswordField registering label={t("New password")} />
            <Notice message={error?.message ?? null} />
            <Button type="submit" pending={submitting}>
              {t("Save new password")}
            </Button>
          </form>
          {error?.gone && (
            <p>
              <Link to="/forgot-password">{t("Ask for a new link")}</Link>
            </p>
          )}
        </>
      )}
    </section>
  );
}
