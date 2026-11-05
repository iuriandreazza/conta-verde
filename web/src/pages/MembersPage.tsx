import { useCallback, useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type Invitation, type Invited, type Member, type Role } from "../api";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { useAuth } from "../auth";
import { UserPlusIcon } from "../components/icons/user-plus";
import { ASSIGNABLE_ROLES, ROLE_HINTS, ROLE_LABELS } from "../lib/roles";

const formatDate = (millis: number) => new Date(millis).toLocaleDateString("pt-BR");

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance.
export function MembersPage() {
  const { companyId = "" } = useParams();
  const { session } = useAuth();
  // Someone who changes or removes themselves here would lose this page under their feet: the owner
  // (or another admin) does it for them.
  const ownId = session.status === "authenticated" ? session.profile.user.id : null;
  const [members, setMembers] = useState<Member[]>([]);
  const [invitations, setInvitations] = useState<Invitation[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [role, setRole] = useState<Role>("viewer");
  const [sending, setSending] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [invited, setInvited] = useState<Invited | null>(null);
  const [revokingId, setRevokingId] = useState<string | null>(null);
  // One member is changed at a time, and removing asks first.
  const [changingId, setChangingId] = useState<string | null>(null);
  const [confirmingId, setConfirmingId] = useState<string | null>(null);
  // Bumped after an invitation so the e-mail field starts over.
  const [formVersion, setFormVersion] = useState(0);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [loadedMembers, loadedInvitations] = await Promise.all([
        api.listMembers(companyId),
        api.listInvitations(companyId),
      ]);
      setMembers(loadedMembers);
      setInvitations(loadedInvitations);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setLoading(false);
    }
  }, [companyId]);

  useEffect(() => {
    void load();
  }, [load]);

  async function handleInvite(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (sending) {
      return;
    }
    const email = String(new FormData(event.currentTarget).get("email") ?? "").trim();
    setSending(true);
    setFormError(null);
    setInvited(null);
    try {
      setInvited(await api.invite(companyId, email, role));
      setFormVersion((version) => version + 1);
      await load();
    } catch (failure) {
      setFormError(errorMessage(failure));
    } finally {
      setSending(false);
    }
  }

  async function changeMember(member: Member, change: () => Promise<void>) {
    if (changingId !== null) {
      return;
    }
    setChangingId(member.user_id);
    setConfirmingId(null);
    setError(null);
    try {
      await change();
      await load();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setChangingId(null);
    }
  }

  async function revoke(invitation: Invitation) {
    if (revokingId !== null) {
      return;
    }
    setRevokingId(invitation.id);
    setError(null);
    try {
      await api.revokeInvitation(companyId, invitation.id);
      // Said about the invitation just made, which may be this one: its link no longer works.
      setInvited((current) => (current?.id === invitation.id ? null : current));
      await load();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setRevokingId(null);
    }
  }

  return (
    <>
      <PageHeader
        title="Membros"
        description="Quem participa desta empresa e o que cada um pode fazer."
      />
      <div className="split">
        <div className="stack">
          <section className="card list-card">
            <h2>Participantes</h2>
            <Notice message={error} />
            {loading ? (
              <p role="status" className="status">
                <Spinner /> Carregando…
              </p>
            ) : (
              <div className="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>Nome</th>
                      <th>E-mail</th>
                      <th>Papel</th>
                      <th>
                        <span className="visually-hidden">Ações</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {members.map((member) => (
                      <tr key={member.user_id}>
                        <td>{member.name}</td>
                        <td>{member.email}</td>
                        {member.role === "owner" || member.user_id === ownId ? (
                          // A company has one owner, who is not changed or removed.
                          <>
                            <td>{ROLE_LABELS[member.role]}</td>
                            <td />
                          </>
                        ) : (
                          <>
                            <td>
                              <select
                                aria-label={`Papel de ${member.name}`}
                                value={member.role}
                                disabled={changingId !== null}
                                onChange={(event) =>
                                  void changeMember(member, () =>
                                    api.changeRole(companyId, member.user_id, event.target.value as Role),
                                  )
                                }
                              >
                                {ASSIGNABLE_ROLES.map((option) => (
                                  <option key={option} value={option}>
                                    {ROLE_LABELS[option]}
                                  </option>
                                ))}
                              </select>
                            </td>
                            <td>
                              {confirmingId === member.user_id ? (
                                <span className="form-actions">
                                  <Button
                                    type="button"
                                    variant="ghost"
                                    className="btn-small"
                                    aria-label={`Confirmar a remoção de ${member.name}`}
                                    pending={changingId === member.user_id}
                                    disabled={changingId !== null}
                                    onClick={() =>
                                      void changeMember(member, () =>
                                        api.removeMember(companyId, member.user_id),
                                      )
                                    }
                                  >
                                    Confirmar
                                  </Button>
                                  <Button
                                    type="button"
                                    variant="ghost"
                                    className="btn-small"
                                    disabled={changingId !== null}
                                    onClick={() => setConfirmingId(null)}
                                  >
                                    Cancelar
                                  </Button>
                                </span>
                              ) : (
                                <Button
                                  type="button"
                                  variant="ghost"
                                  className="btn-small"
                                  aria-label={`Remover ${member.name}`}
                                  disabled={changingId !== null}
                                  onClick={() => setConfirmingId(member.user_id)}
                                >
                                  Remover
                                </Button>
                              )}
                            </td>
                          </>
                        )}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>

          <section className="card list-card" aria-labelledby="open-invitations">
            <h2 id="open-invitations">Convites em aberto</h2>
            {invitations.length === 0 ? (
              <p className="muted">Nenhum convite em aberto.</p>
            ) : (
              <div className="table-wrap">
                <table>
                  <thead>
                    <tr>
                      <th>E-mail</th>
                      <th>Papel</th>
                      <th>Validade</th>
                      <th>
                        <span className="visually-hidden">Ações</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {invitations.map((invitation) => (
                      <tr key={invitation.id}>
                        <td>{invitation.email}</td>
                        <td>{ROLE_LABELS[invitation.role]}</td>
                        <td>
                          {invitation.expired ? (
                            <span className="badge badge-warning">Expirado</span>
                          ) : (
                            `até ${formatDate(invitation.expires_at)}`
                          )}
                        </td>
                        <td>
                          <Button
                            type="button"
                            variant="ghost"
                            className="btn-small"
                            aria-label={`Retirar o convite de ${invitation.email}`}
                            pending={revokingId === invitation.id}
                            disabled={revokingId !== null}
                            onClick={() => void revoke(invitation)}
                          >
                            Retirar
                          </Button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
            <p className="muted">
              Um convite expirado ou retirado não vale mais: convide de novo para mandar outro link.
            </p>
          </section>
        </div>

        <aside className="card form-card">
          <h2>Convidar alguém</h2>
          <form onSubmit={handleInvite} key={formVersion}>
            <label>
              E-mail
              <input
                name="email"
                type="email"
                required
                autoComplete="off"
                autoFocus={formVersion > 0}
              />
            </label>
            <label>
              Papel
              <select value={role} onChange={(event) => setRole(event.target.value as Role)}>
                {ASSIGNABLE_ROLES.map((option) => (
                  <option key={option} value={option}>
                    {ROLE_LABELS[option]}
                  </option>
                ))}
              </select>
            </label>
            <p className="muted" aria-live="polite">
              {ROLE_HINTS[role]}
            </p>
            <Notice message={formError} />
            <Button type="submit" icon={UserPlusIcon} pending={sending}>
              Convidar
            </Button>
          </form>

          {invited && (
            <div className="invite-result">
              <Notice
                tone={invited.emailed ? "success" : "warning"}
                message={
                  invited.emailed
                    ? `Convite enviado para ${invited.email}.`
                    : `Convite criado, mas o e-mail não foi enviado. Passe o link para ${invited.email}.`
                }
              />
              <label>
                Link do convite
                <input
                  readOnly
                  value={invited.link}
                  onFocus={(event) => event.currentTarget.select()}
                />
              </label>
              <p className="muted">
                Este link só aparece agora e só vale para quem entrar com {invited.email}. Vale até{" "}
                {formatDate(invited.expires_at)}.
              </p>
            </div>
          )}
        </aside>
      </div>
    </>
  );
}
