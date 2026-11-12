import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { errorMessage, type Mailed, type SentMail } from "../api";
import { useT } from "../i18n";
import { Button } from "./Button";
import { SendIcon } from "./icons/send";
import { Notice } from "./Notice";

interface MailPanelProps {
  /** What is sent, already translated, as it reads in "Send {what} by e-mail": "the order", "the receipt". */
  what: string;
  /** Sends it to the address, or to the one the register has when it is empty. */
  send: (to: string) => Promise<Mailed>;
  /** What was already sent of it, newest first. */
  loadHistory: () => Promise<SentMail[]>;
  onClose: () => void;
}

/**
 * Sends a document by e-mail to the address on the party's record or to another one typed here. It says what happened:
 * taken for sending, not sent because no mail server is set up, or why it was refused (no address anywhere, say).
 */
export function MailPanel({ what, send, loadHistory, onClose }: MailPanelProps) {
  const t = useT();
  const [sending, setSending] = useState(false);
  const [history, setHistory] = useState<SentMail[]>([]);
  // The record is a help, not a gate: when it cannot be read the panel still sends.
  const loader = useRef(loadHistory);
  loader.current = loadHistory;
  const refreshHistory = useCallback(() => {
    loader
      .current()
      .then(setHistory)
      .catch(() => setHistory([]));
  }, []);
  useEffect(refreshHistory, [refreshHistory]);
  // One send per opening: another click would queue the same document again.
  const [done, setDone] = useState(false);
  const [result, setResult] = useState<{ message: string; tone: "success" | "warning" | "error" } | null>(null);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const to = String(new FormData(event.currentTarget).get("to") ?? "").trim();
    setSending(true);
    setResult(null);
    try {
      const mailed = await send(to);
      setDone(mailed.emailed);
      refreshHistory();
      setResult(
        mailed.emailed
          ? { message: t("Sent to {to}.", { to: mailed.to }), tone: "success" }
          : { message: t("Nothing was sent: the mail server is not set up."), tone: "warning" },
      );
    } catch (failure) {
      setResult({ message: errorMessage(failure), tone: "error" });
    } finally {
      setSending(false);
    }
  }

  return (
    <form className="settlements" onSubmit={handleSubmit}>
      {history.length > 0 && (
        <p className="muted">
          {t("Already sent: {sent}.", {
            sent: history.map((mail) => t("{moment} to {to}", { moment: formatMoment(mail.sent_at_millis), to: mail.to })).join("; "),
          })}
        </p>
      )}
      <label>
        {t("Send to")}
        <input name="to" type="email" autoComplete="off" placeholder={t("Leave empty to use the e-mail on the record")} />
      </label>
      <Notice message={result?.message ?? null} tone={result?.tone} />
      <div className="form-actions">
        <Button type="submit" icon={SendIcon} pending={sending} disabled={done}>
          {t("Send {what} by e-mail", { what })}
        </Button>
        <Button type="button" variant="ghost" disabled={sending} onClick={onClose}>
          {t("Close")}
        </Button>
      </div>
    </form>
  );
}

const MOMENT = new Intl.DateTimeFormat("pt-BR", { dateStyle: "short", timeStyle: "short" });

/** A moment as people read it where they are: `06/11/2026 14:30`. */
function formatMoment(millis: number): string {
  return MOMENT.format(new Date(millis)).replace(",", "");
}
