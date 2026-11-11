import { useEffect, useState } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import { ApiError, api, errorMessage, type InvitationPreview } from "../api";
import { useAuth } from "../auth";
import { Button } from "../components/Button";
import { CheckIcon } from "../components/icons/check";
import { Notice } from "../components/Notice";
import { Spinner } from "../components/Spinner";
import { ROLE_LABELS } from "../lib/roles";
import { useT } from "../i18n";

type Loaded =
  | { status: "loading" }
  | { status: "ready"; preview: InvitationPreview }
  | { status: "failed"; gone: boolean; message: string };

/**
 * Where an invitation's link leads. It says what the invitation is without a session, since the person
 * may not have an account yet; accepting needs the account of the address it was sent to.
 */
export function AcceptInvitationPage() {
  const t = useT();
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
        <Spinner /> Carregando…
      </p>
    );
  }
  if (loaded.status === "failed") {
    return (
      <section className="card narrow">
        <h1>{loaded.gone ? "Convite indisponível" : "Convite não encontrado"}</h1>
        <Notice message={loaded.message} tone="warning" />
        <p>
          <Link to="/">Voltar ao início</Link>
        </p>
      </section>
    );
  }

  const { preview } = loaded;
  const next = encodeURIComponent(`/invitations/${token}`);
  return (
    <section className="card narrow">
      <h1>Convite para {preview.company_name}</h1>
      <p>
        Você foi convidado para participar de <strong>{preview.company_name}</strong> como{" "}
        <strong>{t(ROLE_LABELS[preview.role])}</strong>. O convite é para{" "}
        <strong>{preview.email}</strong>.
      </p>
      {session.status === "authenticated" ? (
        <>
          <p className="muted">
            Você está como {session.profile.user.email}.
            {session.profile.user.email !== preview.email &&
              " Para aceitar, saia e entre com o e-mail do convite."}
          </p>
          <Notice message={error} />
          <Button type="button" icon={CheckIcon} pending={accepting} onClick={() => void accept()}>
            Aceitar convite
          </Button>
        </>
      ) : (
        <>
          <p>Entre, ou crie uma conta, com o e-mail do convite para aceitar.</p>
          <div className="form-actions">
            <Link className="btn btn-primary" to={`/login?next=${next}`}>
              Entrar
            </Link>
            <Link className="btn btn-ghost" to={`/register?next=${next}`}>
              Criar conta
            </Link>
          </div>
        </>
      )}
    </section>
  );
}
