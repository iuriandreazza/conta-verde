import { useState } from "react";
import { Link, useParams } from "react-router-dom";
import { ApiError, api, errorMessage } from "../api";
import { useAuth } from "../auth";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { usePageTitle } from "../lib/usePageTitle";

/**
 * Where the link of the confirmation e-mail leads. Confirming is a button, not something the page does on its own: a
 * mail program that opens links to check them must not use up the person's link.
 */
export function ConfirmEmailPage() {
  const { token = "" } = useParams();
  usePageTitle("Confirmar e-mail");
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
      const message = failure instanceof ApiError && failure.status === 410 ? "Este link venceu ou já foi usado." : errorMessage(failure);
      setState({ failed: message });
    }
  }

  return (
    <section className="card narrow">
      <h1>Confirmar e-mail</h1>
      {state === "done" ? (
        <>
          <Notice tone="success" message="E-mail confirmado. Obrigado!" />
          <p>
            <Link to="/">Ir para o início</Link>
          </p>
        </>
      ) : (
        <>
          <p className="page-lead">Confirme que este endereço de e-mail é seu.</p>
          {typeof state === "object" && <Notice message={state.failed} />}
          <Button type="button" pending={state === "sending"} onClick={() => void confirm()}>
            Confirmar meu e-mail
          </Button>
          {typeof state === "object" && (
            <p>
              <Link to="/">Entrar para pedir um novo link</Link>
            </p>
          )}
        </>
      )}
    </section>
  );
}
