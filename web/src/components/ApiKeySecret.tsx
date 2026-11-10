import { CopyButton } from "./CopyButton";
import { Notice } from "./Notice";

/** What the person must read once a key exists: its secret is shown only now, and the address to use it with. */
export function ApiKeySecret({ name, secret, title }: { name: string; secret: string; title: string }) {
  const base = `${window.location.origin}/api/v1`;
  return (
    <section className="form-section invite-result" aria-labelledby="api-key-done">
      <h3 id="api-key-done">{title}</h3>
      <Notice tone="warning" message="Copie a chave agora: ela não aparece de novo. Se perder, gire a chave para ter outra." />
      <label>
        Chave de “{name}”
        <input readOnly value={secret} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p>
        <CopyButton text={secret} label={`Copiar a chave de ${name}`} />
      </p>
      <label>
        Endereço da API
        <input readOnly value={base} onFocus={(event) => event.currentTarget.select()} />
      </label>
      <p className="muted">
        Envie a chave em cada pedido, no cabeçalho <code>Authorization: Bearer …</code>. A documentação está em{" "}
        <a href="/api/v1/openapi.json" target="_blank" rel="noreferrer">
          /api/v1/openapi.json
        </a>
        .
      </p>
    </section>
  );
}
