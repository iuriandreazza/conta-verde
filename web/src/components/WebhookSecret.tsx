import { CopyButton } from "./CopyButton";
import { Notice } from "./Notice";

/** What the person must read once a secret was made here: it is shown only now. */
export function WebhookSecret({ secret, title }: { secret: string; title: string }) {
  return (
    <section className="form-section invite-result" aria-labelledby="webhook-secret-done">
      <h3 id="webhook-secret-done">{title}</h3>
      <Notice tone="warning" message="Copie o segredo agora: ele não aparece de novo. Se perder, troque o segredo." />
      <label>
        Segredo de assinatura
        <input readOnly value={secret} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p>
        <CopyButton text={secret} label="Copiar o segredo de assinatura" />
      </p>
      <p className="muted">
        Cada entrega leva os cabeçalhos <code>Webhook-Id</code>, <code>Webhook-Timestamp</code> e <code>Webhook-Signature</code>
        (<code>v1,</code> e o HMAC-SHA256 de <code>id.timestamp.corpo</code> em base64, com este texto como chave). Verifique a assinatura, recuse
        timestamps com mais de cinco minutos e guarde os ids já vistos.
      </p>
    </section>
  );
}
