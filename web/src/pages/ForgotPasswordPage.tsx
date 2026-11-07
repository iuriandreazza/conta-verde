import { useState, type FormEvent } from "react";
import { Link } from "react-router-dom";
import { api, errorMessage } from "../api";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { usePageTitle } from "../lib/usePageTitle";

/**
 * Asks for the link that chooses a new password. The answer is the same for any address, so the page says what
 * happens rather than whether the address has an account.
 */
export function ForgotPasswordPage() {
  usePageTitle("Esqueci minha senha");
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
      <h1>Esqueci minha senha</h1>
      {asked !== null ? (
        <>
          <Notice
            tone="success"
            message={`Se ${asked} tiver uma conta, enviamos um link para escolher uma nova senha. Ele vale por 1 hora e só funciona uma vez.`}
          />
          <p>
            <Link to="/login">Voltar para entrar</Link>
          </p>
        </>
      ) : (
        <>
          <p className="page-lead">Informe o e-mail da sua conta e enviamos um link para escolher uma nova senha.</p>
          <form onSubmit={handleSubmit}>
            <label>
              E-mail
              <input name="email" type="email" required autoComplete="email" />
            </label>
            <Notice message={error} />
            <Button type="submit" pending={submitting}>
              Enviar link
            </Button>
          </form>
          <p>
            <Link to="/login">Voltar para entrar</Link>
          </p>
        </>
      )}
    </section>
  );
}
