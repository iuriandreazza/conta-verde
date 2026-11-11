import { motion, useAnimationControls } from "motion/react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link, Navigate, useSearchParams } from "react-router-dom";
import { ApiError, api, errorMessage, type InvitationPreview } from "../api";
import { useAuth } from "../auth";
import { Notice } from "../components/Notice";
import { Button } from "../components/Button";
import { LogInIcon } from "../components/icons/login";
import { UserPlusIcon } from "../components/icons/user-plus";
import { PasswordField } from "../components/PasswordField";
import { Spinner } from "../components/Spinner";
import { ROLE_LABELS } from "../lib/roles";
import { useI18n } from "../i18n";
import { usePageTitle } from "../lib/usePageTitle";

/**
 * Where to go after signing in, when the page was opened from a link that needs an account: an invitation's page or a
 * page of a company (the link of an e-mail), so a crafted `next` cannot send anyone anywhere else.
 */
const NEXT_AFTER_SIGN_IN = [
  /^\/invitations\/[A-Za-z0-9_-]+$/,
  /^\/companies\/[0-9a-f-]{36}\/[a-z-]+(\?[A-Za-z0-9=&_-]*)?$/,
];

function nextAfterSignIn(raw: string | null): string {
  return raw !== null && NEXT_AFTER_SIGN_IN.some((allowed) => allowed.test(raw)) ? raw : "/";
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
  const { t, rich } = useI18n();
  const [params] = useSearchParams();
  const next = nextAfterSignIn(params.get("next"));
  const [error, setError] = useState<string | null>(null);
  // An address that is already taken: the way out is to sign in or to recover the account, not to try another form.
  const [addressTaken, setAddressTaken] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const shake = useAnimationControls();
  const registering = mode === "register";
  usePageTitle(registering ? t("Create account") : t("Sign in"));
  const invitation = useInvitation(registering ? invitationToken(next) : null);
  // What the sign-up made, so that the person is not sent back to an invitation that is used up or
  // was never usable: one who joined is a member of that company and of no other, and goes there; one
  // who made a company of their own goes home.
  const signedUp = useRef<"joined" | "created" | null>(null);

  if (session.status === "authenticated") {
    const [first] = session.profile.companies;
    const destination =
      signedUp.current === "joined" ? `/companies/${first.id}` : signedUp.current === "created" ? "/" : next;
    return <Navigate to={destination} replace />;
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "");
    setSubmitting(true);
    setError(null);
    setAddressTaken(false);
    try {
      if (registering) {
        const account = { name: field("name"), email: field("email"), password: field("password") };
        if (invitation.status === "ready") {
          signedUp.current = "joined";
          await signUp({ ...account, invitation: invitationToken(next) ?? "" });
        } else {
          signedUp.current = "created";
          await signUp({ ...account, company_name: field("company_name") });
        }
      } else {
        await signIn(field("email"), field("password"));
      }
    } catch (failure) {
      signedUp.current = null;
      setError(errorMessage(failure));
      setAddressTaken(registering && failure instanceof ApiError && failure.status === 409);
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
      <h1>{registering ? t("Create your account") : t("Sign in")}</h1>
      {invitation.status === "ready" ? (
        <p className="page-lead">
          {rich("You were invited to {company} as {role}. Create your account with the invitation's e-mail to join it.", {
            company: <strong>{invitation.preview.company_name}</strong>,
            role: <strong>{t(ROLE_LABELS[invitation.preview.role])}</strong>,
          })}
        </p>
      ) : (
        <p className="page-lead">
          {registering ? t("It takes a minute and asks for no card.") : t("Good to see you again.")}
        </p>
      )}
      {invitation.status === "unavailable" && <Notice message={invitation.message} tone="warning" />}
      {invitation.status === "loading" ? (
        // The form waits for the invitation so that nothing typed is lost when the address is fixed.
        <p role="status" className="status">
          <Spinner /> {t("Loading the invitation…")}
        </p>
      ) : (
        <motion.form onSubmit={handleSubmit} animate={shake}>
          {registering && (
            <label>
              {t("Your name")}
              <input name="name" required autoComplete="name" />
            </label>
          )}
          {registering && invitation.status !== "ready" && (
            <label>
              {t("Company name")}
              <input name="company_name" required autoComplete="organization" />
            </label>
          )}
          <label>
            {t("E-mail")}
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
          {addressTaken && (
            <p>
              {rich("This e-mail already has an account. {signIn} or {recover}.", {
                signIn: <Link to="/login">{t("Sign in")}</Link>,
                recover: <Link to="/forgot-password">{t("recover the password")}</Link>,
              })}
            </p>
          )}
          <Button type="submit" icon={registering ? UserPlusIcon : LogInIcon} pending={submitting}>
            {registering ? t("Create account") : t("Sign in")}
          </Button>
        </motion.form>
      )}
      {!registering && (
        <p>
          <Link to="/forgot-password">{t("I forgot my password")}</Link>
        </p>
      )}
      <p>
        {registering ? (
          <>
            {rich("Already have an account? {link}", { link: <Link to={withNext("/login", next)}>{t("Sign in")}</Link> })}
          </>
        ) : (
          <>
            {rich("First time here? {link}", { link: <Link to={withNext("/register", next)}>{t("Create account")}</Link> })}
          </>
        )}
      </p>
    </motion.section>
  );
}
