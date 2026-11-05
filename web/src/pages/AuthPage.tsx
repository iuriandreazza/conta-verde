import { motion, useAnimationControls } from "motion/react";
import { useState, type FormEvent } from "react";
import { Link, Navigate } from "react-router-dom";
import { errorMessage } from "../api";
import { useAuth } from "../auth";
import { Notice } from "../components/Notice";
import { Button } from "../components/Button";
import { LogInIcon } from "../components/icons/login";
import { UserPlusIcon } from "../components/icons/user-plus";
import { PasswordField } from "../components/PasswordField";

export function AuthPage({ mode }: { mode: "login" | "register" }) {
  const { session, signIn, signUp } = useAuth();
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const shake = useAnimationControls();
  const registering = mode === "register";

  if (session.status === "authenticated") {
    return <Navigate to="/" replace />;
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "");
    setSubmitting(true);
    setError(null);
    try {
      if (registering) {
        await signUp({
          name: field("name"),
          email: field("email"),
          password: field("password"),
          company_name: field("company_name"),
        });
      } else {
        await signIn(field("email"), field("password"));
      }
    } catch (failure) {
      setError(errorMessage(failure));
      void shake.start({ x: [0, -8, 8, -5, 5, 0], transition: { duration: 0.4 } });
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <motion.section
      className="card narrow"
      initial={{ opacity: 0, y: 16, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      transition={{ type: "spring", stiffness: 260, damping: 24 }}
    >
      <h1>{registering ? "Crie sua conta" : "Entrar"}</h1>
      <p className="page-lead">
        {registering ? "Leva um minuto e não pede cartão." : "Que bom ver você de novo."}
      </p>
      <motion.form onSubmit={handleSubmit} animate={shake}>
        {registering && (
          <>
            <label>
              Seu nome
              <input name="name" required autoComplete="name" />
            </label>
            <label>
              Nome da empresa
              <input name="company_name" required autoComplete="organization" />
            </label>
          </>
        )}
        <label>
          E-mail
          <input name="email" type="email" required autoComplete="email" />
        </label>
        <PasswordField registering={registering} />
        <Notice message={error} />
        <Button type="submit" icon={registering ? UserPlusIcon : LogInIcon} pending={submitting}>
          {registering ? "Criar conta" : "Entrar"}
        </Button>
      </motion.form>
      <p>
        {registering ? (
          <>
            Já tem conta? <Link to="/login">Entrar</Link>
          </>
        ) : (
          <>
            Primeira vez aqui? <Link to="/register">Criar conta</Link>
          </>
        )}
      </p>
    </motion.section>
  );
}
