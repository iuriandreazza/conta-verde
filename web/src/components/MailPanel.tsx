import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { errorMessage, type Mailed, type SentMail } from "../api";
import { Button } from "./Button";
import { SendIcon } from "./icons/send";
import { Notice } from "./Notice";

interface MailPanelProps {
  /** What is sent, as it reads in "Enviar {what} por e-mail": "o pedido", "o comprovante". */
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
          ? { message: `Enviado para ${mailed.to}.`, tone: "success" }
          : { message: "Nada foi enviado: o servidor de e-mail não está configurado.", tone: "warning" },
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
          Já enviado: {history.map((mail) => `${formatMoment(mail.sent_at_millis)} para ${mail.to}`).join("; ")}.
        </p>
      )}
      <label>
        Enviar para
        <input
          name="to"
          type="email"
          autoComplete="off"
          placeholder="Deixe vazio para usar o e-mail do cadastro"
        />
      </label>
      <Notice message={result?.message ?? null} tone={result?.tone} />
      <div className="form-actions">
        <Button type="submit" icon={SendIcon} pending={sending} disabled={done}>
          Enviar {what} por e-mail
        </Button>
        <Button type="button" variant="ghost" disabled={sending} onClick={onClose}>
          Fechar
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
