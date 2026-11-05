import type { DocumentCheck } from "../api";

/** Shows what the federal registry said about a CNPJ next to it; nothing for a CPF or no document. */
export function DocumentBadge({ check }: { check: DocumentCheck }) {
  if (check.status === "not_applicable") {
    return null;
  }
  if (check.status === "unverified") {
    return (
      <span className="badge badge-muted" title="A Receita não pôde ser consultada ao cadastrar">
        Não verificado
      </span>
    );
  }
  return (
    <span
      className={check.needs_attention ? "badge badge-warning" : "badge badge-ok"}
      title={check.legal_name ?? undefined}
    >
      {check.needs_attention ? check.situation : "Verificado"}
    </span>
  );
}
