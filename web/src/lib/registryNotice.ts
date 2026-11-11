import type { Party } from "../api";
import type { NoticeTone } from "../components/Notice";
import type { Translate } from "../i18n";

export interface RegistryNotice {
  message: string;
  tone: NoticeTone;
}

// The party is already saved when this runs: the notice only reports what the registry said,
// so a warning never asks the user to redo anything. `after` says which action produced the
// answer, since "Registered" would be wrong for a re-check of an old row.
export function registryNoticeFor(
  party: Party,
  after: "registering" | "updating" | "rechecking",
  t: Translate,
): RegistryNotice | null {
  const check = party.document_check;
  const rechecking = after === "rechecking";
  const updating = after === "updating";
  const named = t("{name}:", { name: party.name });
  const lead = rechecking ? named : updating ? t("Updated, but") : t("Registered, but");
  if (check.status === "not_found") {
    return {
      tone: "warning",
      message: `${lead} ${t("the federal registry did not find this CNPJ. Check the number: it may take a while to include newly opened companies.")}`,
    };
  }
  if (check.status === "unverified") {
    return {
      tone: "warning",
      message: rechecking
        ? t("{name}: the federal registry is still unavailable. Try again later.", { name: party.name })
        : `${lead} ${t("the CNPJ could not be checked with the federal registry now. Only the digits were checked.")}`,
    };
  }
  if (check.status === "verified") {
    const situation = t("{legal_name} has the status {situation} with the federal registry.", {
      legal_name: String(check.legal_name),
      situation: String(check.situation),
    });
    const attention = rechecking ? named : updating ? t("Updated. Attention:") : t("Registered. Attention:");
    return check.needs_attention
      ? { tone: "warning", message: `${attention} ${situation}` }
      : {
          tone: "success",
          message: `${rechecking ? `${named} ` : ""}${t("CNPJ verified: {legal_name}.", { legal_name: String(check.legal_name) })}`,
        };
  }
  return null;
}
