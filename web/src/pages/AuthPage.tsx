import { useState, type FormEvent } from "react";
import { Link, Navigate } from "react-router-dom";
import { ApiError } from "../api";
import { useAuth } from "../auth";

export function AuthPage({ mode }: { mode: "login" | "register" }) {
  const { session, signIn, signUp } = useAuth();
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
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
      setError(failure instanceof ApiError ? failure.message : "Something went wrong, try again.");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <section className="card narrow">
      <h1>{registering ? "Create your account" : "Sign in"}</h1>
      <form onSubmit={handleSubmit}>
        {registering && (
          <>
            <label>
              Your name
              <input name="name" required autoComplete="name" />
            </label>
            <label>
              Company name
              <input name="company_name" required autoComplete="organization" />
            </label>
          </>
        )}
        <label>
          Email
          <input name="email" type="email" required autoComplete="email" />
        </label>
        <label>
          Password
          <input
            name="password"
            type="password"
            required
            minLength={registering ? 8 : undefined}
            autoComplete={registering ? "new-password" : "current-password"}
          />
        </label>
        {error && <p role="alert" className="error">{error}</p>}
        <button type="submit" disabled={submitting}>
          {registering ? "Create account" : "Sign in"}
        </button>
      </form>
      <p>
        {registering ? (
          <>
            Already have an account? <Link to="/login">Sign in</Link>
          </>
        ) : (
          <>
            New here? <Link to="/register">Create an account</Link>
          </>
        )}
      </p>
    </section>
  );
}
