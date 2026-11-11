import type { DocumentCheck } from "../api";
import { useT } from "../i18n";

/** Shows what the federal registry said about a CNPJ next to it; nothing for a CPF or no document. */
export function DocumentBadge({ check }: { check: DocumentCheck }) {
  const t = useT();
  if (check.status === "not_applicable") {
    return null;
  }
  if (check.status === "not_found") {
    return (
      <span
        className="badge badge-warning"
        title={t("The federal revenue service did not find this CNPJ. Check the number: its data can take a while to include new companies.")}
      >
        {t("Not found")}
      </span>
    );
  }
  if (check.status === "unverified") {
    return (
      <span className="badge badge-muted" title={t("The federal revenue service could not be asked when this was registered")}>
        {t("Not verified")}
      </span>
    );
  }
  return (
    <span
      className={check.needs_attention ? "badge badge-warning" : "badge badge-ok"}
      title={check.legal_name ?? undefined}
    >
      {check.needs_attention ? check.situation : t("Verified")}
      {/* The legal name tells whether a typo landed on another company, so it must not depend on hover. */}
      {check.legal_name && <span className="visually-hidden"> ({check.legal_name})</span>}
    </span>
  );
}
