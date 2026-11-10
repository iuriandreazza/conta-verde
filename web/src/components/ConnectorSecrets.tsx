import { CopyButton } from "./CopyButton";
import { Notice } from "./Notice";

/** What the person must read once a connector was turned on: the two secrets exist only now, and where to put them. */
export function ConnectorSecrets({ name, apiKey, webhookSecret, packageLink }: { name: string; apiKey: string; webhookSecret: string; packageLink: string }) {
  const base = `${window.location.origin}/api/v1`;
  return (
    <section className="form-section invite-result" aria-labelledby="connector-done">
      <h3 id="connector-done">“{name}” ligado</h3>
      <Notice tone="warning" message="Copie a chave e o segredo agora: eles não aparecem de novo. Se perder, desligue e ligue o conector." />
      <label>
        Chave da API
        <input readOnly value={apiKey} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p>
        <CopyButton text={apiKey} label={`Copiar a chave de ${name}`} />
      </p>
      <label>
        Segredo do webhook
        <input readOnly value={webhookSecret} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p>
        <CopyButton text={webhookSecret} label={`Copiar o segredo do webhook de ${name}`} />
      </p>
      <label>
        Endereço da API
        <input readOnly value={base} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p className="muted">
        Cole os três na configuração do pacote, junto com os dados da loja, que ficam só lá. O passo a passo está no{" "}
        <a href={packageLink} target="_blank" rel="noreferrer">
          README do pacote
        </a>
        .
      </p>
    </section>
  );
}
