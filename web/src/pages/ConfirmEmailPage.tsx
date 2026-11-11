import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { ApiError, api, errorMessage } from "../api";
import { useAuth } from "../auth";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { useT } from "../i18n";
import { usePageTitle } from "../lib/usePageTitle";

/**
 * Where the link of the confirmation e-mail leads. Confirming is a button, not something the page does on its own: a
 * mail program that opens links to check them must not use up the person's link.
 */
export function ConfirmEmailPage() {
  const t = useT();
  const { token = "" } = useParams();
  usePageTitle(t("Confirm e-mail"));
  const { refresh } = useAuth();
  const [state, setState] = useState<"idle" | "sending" | "done" | { failed: string }>("idle");

  async function confirm() {
    setState("sending");
    try {
      await api.confirmEmail(token);
      setState("done");
      // A browser that is signed in stops asking for the confirmation; one that is not has nothing to refresh.
      refresh().catch(() => undefined);
    } catch (failure) {
      const message = failure instanceof ApiError && failure.status === 410 ? t("This link has expired or was already used.") : errorMessage(failure);
      setState({ failed: message });
    }
  }

  return (
    <section className="card narrow">
      <h1>{t("Confirm e-mail")}</h1>
      {state === "done" ? (
        <>
          <Notice tone="success" message={t("E-mail confirmed. Thank you!")} />
          <p>
            <Link to="/">{t("Go to the start")}</Link>
          </p>
        </>
      ) : (
        <>
          <p className="page-lead">{t("Confirm that this e-mail address is yours.")}</p>
          {typeof state === "object" && <Notice message={state.failed} />}
          <Button type="button" pending={state === "sending"} onClick={() => void confirm()}>
            {t("Confirm my e-mail")}
          </Button>
          {typeof state === "object" && (
            <p>
              <Link to="/">{t("Sign in to ask for a new link")}</Link>
            </p>
          )}
        </>
      )}
    </section>
  );
}
