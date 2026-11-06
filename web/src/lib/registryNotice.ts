import type { Party } from "../api";
import type { NoticeTone } from "../components/Notice";

export interface RegistryNotice {
  message: string;
  tone: NoticeTone;
}

// The party is already saved when this runs: the notice only reports what the registry said,
// so a warning never asks the user to redo anything. `after` says which action produced the
// answer, since "Cadastrado" would be wrong for a re-check of an old row.
export function registryNoticeFor(
  party: Party,
  after: "registering" | "updating" | "rechecking",
): RegistryNotice | null {
  const check = party.document_check;
  const rechecking = after === "rechecking";
  const done = after === "updating" ? "Atualizado" : "Cadastrado";
  const lead = rechecking ? `${party.name}:` : `${done}, mas`;
  if (check.status === "not_found") {
    return {
      tone: "warning",
      message: `${lead} a Receita não encontrou este CNPJ. Confira o número: ela pode demorar a incluir empresas recém-abertas.`,
    };
  }
  if (check.status === "unverified") {
    return {
      tone: "warning",
      message: rechecking
        ? `${party.name}: a Receita continua indisponível. Tente de novo mais tarde.`
        : `${done}, mas não foi possível consultar o CNPJ na Receita agora. Só os dígitos foram conferidos.`,
    };
  }
  if (check.status === "verified") {
    return check.needs_attention
      ? {
          tone: "warning",
          message: `${rechecking ? `${party.name}:` : `${done}. Atenção:`} ${check.legal_name} está com situação ${check.situation} na Receita.`,
        }
      : {
          tone: "success",
          message: `${rechecking ? `${party.name}: ` : ""}CNPJ verificado: ${check.legal_name}.`,
        };
  }
  return null;
}
