import { useState } from "react";
import { api, errorMessage } from "../api";
import { useT } from "../i18n";
import { Button } from "./Button";

/**
 * Asks a signed-in person to confirm the address of their account, until they do. The link is in the e-mail sent when
 * the account was made; this offers to send it again (one a minute at most is made, so a second click may change nothing).
 */
export function EmailConfirmationBanner({ email }: { email: string }) {
  const t = useT();
  const [state, setState] = useState<{ kind: "idle" | "sending" | "sent" } | { kind: "failed"; message: string }>({
    kind: "idle",
  });

  async function resend() {
    setState({ kind: "sending" });
    try {
      await api.requestEmailConfirmation();
      setState({ kind: "sent" });
    } catch (failure) {
      setState({ kind: "failed", message: errorMessage(failure) });
    }
  }

  return (
    <div className="banner" role="status">
      <span>
        {state.kind === "sent"
          ? t("We sent a new link to {email}.", { email })
          : state.kind === "failed"
            ? state.message
            : t("Confirm your e-mail: we sent a link to {email}.", { email })}
      </span>
      {state.kind !== "sent" && (
        <Button type="button" variant="ghost" className="btn-small" pending={state.kind === "sending"} onClick={() => void resend()}>
          {t("Resend link")}
        </Button>
      )}
    </div>
  );
}
