import { useCallback, useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { api, errorMessage, type Invitation, type Member, type Role } from "../api";
import { Button } from "../components/Button";
import { Notice } from "../components/Notice";
import { PageHeader } from "../components/PageHeader";
import { Spinner } from "../components/Spinner";
import { useAuth } from "../auth";
import { useCreation, useOnCreated } from "../creation/CreationProvider";
import { useCanWrite } from "../lib/useCanWrite";
import { UserPlusIcon } from "../components/icons/user-plus";
import { ASSIGNABLE_ROLES, ROLE_LABELS } from "../lib/roles";

const formatDate = (millis: number) => new Date(millis).toLocaleDateString("pt-BR");

// Layout remounts the page whenever the path changes, so `companyId` never changes within one
// instance.
export function MembersPage() {
  const { companyId = "" } = useParams();
  const { session } = useAuth();
  const canWrite = useCanWrite("members");
  // Someone who changes or removes themselves here would lose this page under their feet: the owner
  // (or another admin) does it for them.
  const ownId = session.status === "authenticated" ? session.profile.user.id : null;
  const [members, setMembers] = useState<Member[]>([]);
  const [invitations, setInvitations] = useState<Invitation[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [revokingId, setRevokingId] = useState<string | null>(null);
  // One member is changed at a time, and removing asks first.
  const [changingId, setChangingId] = useState<string | null>(null);
  const [confirmingId, setConfirmingId] = useState<string | null>(null);

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
      // Read again even so: a refusal often means the list was out of date (the member was removed
      // meanwhile). `load` clears the error, so it is set after.
      await load();
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
      await load();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setRevokingId(null);
    }
  }

  const { openCreate } = useCreation();
  // An invitation made from the top bar or from here: read the lists again.
  useOnCreated("invite", () => void load());

  return (
    <>
      <PageHeader
        title="Membros"
        description="Quem participa desta empresa e o que cada um pode fazer."
      />
      {canWrite && (
        <div className="list-actions">
          <Button type="button" icon={UserPlusIcon} onClick={() => openCreate("invite")}>
            Convidar alguém
          </Button>
        </div>
      )}
      <div>
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

      </div>
    </>
  );
}
