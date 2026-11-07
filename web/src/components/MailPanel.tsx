import { useState, type FormEvent } from "react";
import { errorMessage, type Mailed } from "../api";
import { Button } from "./Button";
import { SendIcon } from "./icons/send";
import { Notice } from "./Notice";

interface MailPanelProps {
  /** What is sent, as it reads in "Enviar {what} por e-mail": "o pedido", "o comprovante". */
  what: string;
  /** Sends it to the address, or to the one the register has when it is empty. */
  send: (to: string) => Promise<Mailed>;
  onClose: () => void;
}

/**
 * Sends a document by e-mail to the address on the party's record or to another one typed here. It says what happened:
 * taken for sending, not sent because no mail server is set up, or why it was refused (no address anywhere, say).
 */
export function MailPanel({ what, send, onClose }: MailPanelProps) {
  const [sending, setSending] = useState(false);
  const [result, setResult] = useState<{ message: string; tone: "success" | "warning" | "error" } | null>(null);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const to = String(new FormData(event.currentTarget).get("to") ?? "").trim();
    setSending(true);
    setResult(null);
    try {
      const mailed = await send(to);
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
        <Button type="submit" icon={SendIcon} pending={sending}>
          Enviar {what} por e-mail
        </Button>
        <Button type="button" variant="ghost" disabled={sending} onClick={onClose}>
          Fechar
        </Button>
      </div>
    </form>
  );
}
