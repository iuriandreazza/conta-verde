import type { QuoteStatus } from "../api";

/** How a quote's status reads to a person; the keys are the API's and never change. */
export const QUOTE_STATUS_LABELS: Record<QuoteStatus, string> = {
  draft: "Rascunho",
  sent: "Enviado",
  approved: "Aprovado",
  declined: "Recusado",
  converted: "Virou venda",
};
