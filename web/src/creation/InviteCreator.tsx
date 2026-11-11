import { useEffect, useState, type FormEvent } from "react";
import { api, errorMessage, type Invited, type Role } from "../api";
import { Button } from "../components/Button";
import { BackButton } from "../components/BackButton";
import { UserPlusIcon } from "../components/icons/user-plus";
import { Notice } from "../components/Notice";
import { ASSIGNABLE_ROLES, ROLE_HINTS, ROLE_LABELS } from "../lib/roles";
import type { CreatorProps } from "./CreationProvider";
import { useT } from "../i18n";

const formatDate = (millis: number) => new Date(millis).toLocaleDateString("pt-BR");

/** What the person must read once the invitation exists: the link is shown only now. */
function InviteResult({ invited }: { invited: Invited }) {
  const t = useT();
  return (
    <section className="form-section invite-result" aria-labelledby="invite-done">
      <h3 id="invite-done">{t("Invitation created")}</h3>
      <Notice
        tone={invited.emailed ? "success" : "warning"}
        message={
          invited.emailed
            ? t("Invitation created: the e-mail to {email} is in the sending queue.", { email: invited.email })
            : t("Invitation created, but the e-mail was not sent. Pass the link on to {email}.", { email: invited.email })
        }
      />
      <label>
        {t("Invitation link")}
        <input readOnly value={invited.link} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p className="muted">
        {t("This link only shows now and is only valid for whoever signs in with {email}. Valid until {date}.", {
          email: invited.email,
          date: formatDate(invited.expires_at),
        })}
      </p>
    </section>
  );
}

/** The form that invites someone to the company, with the role they will have. */
export function InviteCreator({ companyId, onBusyChange, onCreated, onCancel }: CreatorProps) {
  const t = useT();
  const [role, setRole] = useState<Role>("viewer");
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    onBusyChange(sending);
  }, [sending, onBusyChange]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (sending) return;
    const email = String(new FormData(event.currentTarget).get("email") ?? "").trim();
    setSending(true);
    setError(null);
    try {
      const invited = await api.invite(companyId, email, role);
      onCreated({ id: invited.id, message: t("Invitation created for {email}.", { email: invited.email }), result: <InviteResult invited={invited} /> });
    } catch (failure) {
      setError(errorMessage(failure));
      setSending(false);
    }
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="invite-info">
        <h3 id="invite-info">{t("Who joins")}</h3>
        <div className="field-grid">
          <label>
            {t("E-mail")}
            <input name="email" type="email" required autoComplete="off" />
          </label>
          <label>
            {t("Role")}
            <select value={role} onChange={(event) => setRole(event.target.value as Role)}>
              {ASSIGNABLE_ROLES.map((option) => (
                <option key={option} value={option}>
                  {t(ROLE_LABELS[option])}
                </option>
              ))}
            </select>
          </label>
        </div>
        <p className="muted" aria-live="polite">
          {t(ROLE_HINTS[role])}
        </p>
      </section>
      <Notice message={error} />
      <div className="form-footer">
        <BackButton disabled={sending} onClick={onCancel} />
        <Button type="submit" icon={UserPlusIcon} pending={sending}>
          {t("Invite")}
        </Button>
      </div>
    </form>
  );
}
