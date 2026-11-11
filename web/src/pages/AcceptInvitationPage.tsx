import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { ApiError, api, errorMessage, type InvitationPreview } from "../api";
import { useAuth } from "../auth";
import { Button } from "../components/Button";
import { CheckIcon } from "../components/icons/check";
import { Notice } from "../components/Notice";
import { Spinner } from "../components/Spinner";
import { ROLE_LABELS } from "../lib/roles";
import { useI18n } from "../i18n";

type Loaded =
  | { status: "loading" }
  | { status: "ready"; preview: InvitationPreview }
  | { status: "failed"; gone: boolean; message: string };

/**
 * Where an invitation's link leads. It says what the invitation is without a session, since the person
 * may not have an account yet; accepting needs the account of the address it was sent to.
 */
export function AcceptInvitationPage() {
  const { t, rich } = useI18n();
  const { token = "" } = useParams();
  const { session, refresh } = useAuth();
  const navigate = useNavigate();
  const [loaded, setLoaded] = useState<Loaded>({ status: "loading" });
  const [accepting, setAccepting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let current = true;
    api
      .previewInvitation(token)
      .then((preview) => current && setLoaded({ status: "ready", preview }))
      .catch((failure: unknown) => {
        if (current) {
          setLoaded({
            status: "failed",
            gone: failure instanceof ApiError && failure.status === 410,
            message: errorMessage(failure),
          });
        }
      });
    return () => {
      current = false;
    };
  }, [token]);

  async function accept() {
    setAccepting(true);
    setError(null);
    try {
      const { company_id } = await api.acceptInvitation(token);
      // The company is new to the profile: read it again before going there.
      await refresh();
      navigate(`/companies/${company_id}`, { replace: true });
    } catch (failure) {
      setError(errorMessage(failure));
      setAccepting(false);
    }
  }

  if (loaded.status === "loading" || session.status === "loading") {
    return (
      <p role="status" className="status">
        <Spinner /> {t("Loading…")}
      </p>
    );
  }
  if (loaded.status === "failed") {
    return (
      <section className="card narrow">
        <h1>{loaded.gone ? t("Invitation unavailable") : t("Invitation not found")}</h1>
        <Notice message={loaded.message} tone="warning" />
        <p>
          <Link to="/">{t("Back to the start")}</Link>
        </p>
      </section>
    );
  }

  const { preview } = loaded;
  const next = encodeURIComponent(`/invitations/${token}`);
  return (
    <section className="card narrow">
      <h1>{t("Invitation to {company}", { company: preview.company_name })}</h1>
      <p>
        {rich("You were invited to take part in {company} as {role}. The invitation is for {email}.", {
          company: <strong>{preview.company_name}</strong>,
          role: <strong>{t(ROLE_LABELS[preview.role])}</strong>,
          email: <strong>{preview.email}</strong>,
        })}
      </p>
      {session.status === "authenticated" ? (
        <>
          <p className="muted">
            {t("You are signed in as {email}.", { email: session.profile.user.email })}
            {session.profile.user.email !== preview.email && ` ${t("To accept, sign out and sign in with the invitation's e-mail.")}`}
          </p>
          <Notice message={error} />
          <Button type="button" icon={CheckIcon} pending={accepting} onClick={() => void accept()}>
            {t("Accept invitation")}
          </Button>
        </>
      ) : (
        <>
          <p>{t("Sign in, or create an account, with the invitation's e-mail to accept.")}</p>
          <div className="form-actions">
            <Link className="btn btn-primary" to={`/login?next=${next}`}>
              {t("Sign in")}
            </Link>
            <Link className="btn btn-ghost" to={`/register?next=${next}`}>
              {t("Create account")}
            </Link>
          </div>
        </>
      )}
    </section>
  );
}
