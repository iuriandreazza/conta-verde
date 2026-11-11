import { useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import { api, errorMessage } from "../api";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { useT } from "../i18n";
import { usePageTitle } from "../lib/usePageTitle";

/**
 * Asks for the link that chooses a new password. The answer is the same for any address, so the page says what
 * happens rather than whether the address has an account.
 */
export function ForgotPasswordPage() {
  const t = useT();
  usePageTitle(t("I forgot my password"));
  const [asked, setAsked] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const email = String(new FormData(event.currentTarget).get("email") ?? "").trim();
    setSubmitting(true);
    setError(null);
    try {
      await api.requestPasswordReset(email);
      setAsked(email);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <section className="card narrow">
      <h1>{t("I forgot my password")}</h1>
      {asked !== null ? (
        <>
          <Notice
            tone="success"
            message={t("If {email} has an account, we sent a link to choose a new password. It is valid for 1 hour and works only once.", { email: asked })}
          />
          <p>
            <Link to="/login">{t("Back to sign in")}</Link>
          </p>
        </>
      ) : (
        <>
          <p className="page-lead">{t("Enter your account's e-mail and we send a link to choose a new password.")}</p>
          <form onSubmit={handleSubmit}>
            <label>
              {t("E-mail")}
              <input name="email" type="email" required autoComplete="email" />
            </label>
            <Notice message={error} />
            <Button type="submit" pending={submitting}>
              {t("Send link")}
            </Button>
          </form>
          <p>
            <Link to="/login">{t("Back to sign in")}</Link>
          </p>
        </>
      )}
    </section>
  );
}
