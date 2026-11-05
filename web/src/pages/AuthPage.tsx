import { motion, useAnimationControls } from "motion/react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link, Navigate, useSearchParams } from "react-router-dom";
import { api, errorMessage, type InvitationPreview } from "../api";
import { useAuth } from "../auth";
import { Notice } from "../components/Notice";
import { Button } from "../components/Button";
import { LogInIcon } from "../components/icons/login";
import { UserPlusIcon } from "../components/icons/user-plus";
import { PasswordField } from "../components/PasswordField";
import { Spinner } from "../components/Spinner";
import { ROLE_LABELS } from "../lib/roles";

/**
 * Where to go after signing in, when the page was opened from a link that needs an account: only an
 * invitation's page, so a crafted `next` cannot send anyone anywhere else.
 */
function nextAfterSignIn(raw: string | null): string {
  return raw !== null && /^\/invitations\/[A-Za-z0-9_-]+$/.test(raw) ? raw : "/";
}

/** The token of the invitation a `next` leads to, if it leads to one. */
function invitationToken(next: string): string | null {
  return next.startsWith("/invitations/") ? next.slice("/invitations/".length) : null;
}

type Invitation =
  | { status: "none" }
  | { status: "loading" }
  | { status: "ready"; preview: InvitationPreview }
  | { status: "unavailable"; message: string };

/**
 * What the invitation behind a sign-up page says, so the form can ask only what an invited person
 * needs: a company of their own is not one of them. A link that no longer works leaves an ordinary
 * sign-up, with the reason shown, since the account is worth having either way.
 */
function useInvitation(token: string | null): Invitation {
  const [loaded, setInvitation] = useState<Invitation>({ status: "loading" });
  useEffect(() => {
    if (token === null) {
      return;
    }
    let current = true;
    setInvitation({ status: "loading" });
    api
      .previewInvitation(token)
      .then((preview) => current && setInvitation({ status: "ready", preview }))
      .catch((failure: unknown) => current && setInvitation({ status: "unavailable", message: errorMessage(failure) }));
    return () => {
      current = false;
    };
  }, [token]);
  return token === null ? { status: "none" } : loaded;
}

/** The other of the two pages keeps the way back to where the person was going. */
function withNext(path: string, next: string): string {
  return next === "/" ? path : `${path}?next=${encodeURIComponent(next)}`;
}

export function AuthPage({ mode }: { mode: "login" | "register" }) {
  const { session, signIn, signUp } = useAuth();
  const [params] = useSearchParams();
  const next = nextAfterSignIn(params.get("next"));
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const shake = useAnimationControls();
  const registering = mode === "register";
  const invitation = useInvitation(registering ? invitationToken(next) : null);
  // Someone who just joined through the link is a member of that company and of no other: they go
  // there, not back to the invitation, which is used up by now.
  const joinedThroughInvitation = useRef(false);

  if (session.status === "authenticated") {
    const [joined] = session.profile.companies;
    return <Navigate to={joinedThroughInvitation.current ? `/companies/${joined.id}` : next} replace />;
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "");
    setSubmitting(true);
    setError(null);
    try {
      if (registering) {
        const account = { name: field("name"), email: field("email"), password: field("password") };
        if (invitation.status === "ready") {
          joinedThroughInvitation.current = true;
          await signUp({ ...account, invitation: invitationToken(next) ?? "" });
        } else {
          await signUp({ ...account, company_name: field("company_name") });
        }
      } else {
        await signIn(field("email"), field("password"));
      }
    } catch (failure) {
      joinedThroughInvitation.current = false;
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
      {invitation.status === "ready" ? (
        <p className="page-lead">
          Você foi convidado para <strong>{invitation.preview.company_name}</strong> como{" "}
          <strong>{ROLE_LABELS[invitation.preview.role]}</strong>. Crie sua conta com o e-mail do convite
          para entrar nela.
        </p>
      ) : (
        <p className="page-lead">
          {registering ? "Leva um minuto e não pede cartão." : "Que bom ver você de novo."}
        </p>
      )}
      {invitation.status === "unavailable" && <Notice message={invitation.message} tone="warning" />}
      {invitation.status === "loading" ? (
        // The form waits for the invitation so that nothing typed is lost when the address is fixed.
        <p role="status" className="status">
          <Spinner /> Carregando o convite…
        </p>
      ) : (
        <motion.form onSubmit={handleSubmit} animate={shake}>
          {registering && (
            <label>
              Seu nome
              <input name="name" required autoComplete="name" />
            </label>
          )}
          {registering && invitation.status !== "ready" && (
            <label>
              Nome da empresa
              <input name="company_name" required autoComplete="organization" />
            </label>
          )}
          <label>
            E-mail
            <input
              name="email"
              type="email"
              required
              autoComplete="email"
              // Another address would be refused, so the one that is accepted is given.
              {...(invitation.status === "ready" ? { defaultValue: invitation.preview.email, readOnly: true } : {})}
            />
          </label>
          <PasswordField registering={registering} />
          <Notice message={error} />
          <Button type="submit" icon={registering ? UserPlusIcon : LogInIcon} pending={submitting}>
            {registering ? "Criar conta" : "Entrar"}
          </Button>
        </motion.form>
      )}
      <p>
        {registering ? (
          <>
            Já tem conta? <Link to={withNext("/login", next)}>Entrar</Link>
          </>
        ) : (
          <>
            Primeira vez aqui? <Link to={withNext("/register", next)}>Criar conta</Link>
          </>
        )}
      </p>
    </motion.section>
  );
}
