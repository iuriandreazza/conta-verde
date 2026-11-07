import { useState, type FormEvent } from "react";
import { Link, useParams } from "react-router-dom";
import { ApiError, api, errorMessage } from "../api";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { PasswordField } from "../components/PasswordField";
import { usePageTitle } from "../lib/usePageTitle";

/**
 * Where the link of the e-mail leads: a new password for the account. The link works once and every session of the
 * account ends, so the person signs in again after it.
 */
export function ResetPasswordPage() {
  const { token = "" } = useParams();
  usePageTitle("Nova senha");
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
    } catch (failure) {
      setError({ message: errorMessage(failure), gone: failure instanceof ApiError && failure.status === 410 });
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <section className="card narrow">
      <h1>Nova senha</h1>
      {done ? (
        <>
          <Notice tone="success" message="Senha alterada. Entre com a nova senha." />
          <p>
            <Link to="/login">Entrar</Link>
          </p>
        </>
      ) : (
        <>
          <p className="page-lead">Escolha a nova senha da sua conta.</p>
          <form onSubmit={handleSubmit}>
            <PasswordField registering label="Nova senha" />
            <Notice message={error?.message ?? null} />
            <Button type="submit" pending={submitting}>
              Salvar nova senha
            </Button>
          </form>
          {error?.gone && (
            <p>
              <Link to="/forgot-password">Pedir um novo link</Link>
            </p>
          )}
        </>
      )}
    </section>
  );
}
