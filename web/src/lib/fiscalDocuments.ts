import type { BuyerPresence, FiscalDocumentStatus, FiscalEventStatus, PaymentForm } from "../api";

export const STATUS_LABELS: Record<FiscalDocumentStatus, string> = {
  draft: "Rascunho",
  processing: "Processando",
  authorized: "Autorizada",
  contingency: "Em contingência",
  rejected: "Rejeitada",
  denied: "Denegada",
  cancelled: "Cancelada",
};

export const EVENT_STATUS_LABELS: Record<FiscalEventStatus, string> = {
  sent: "Enviada, sem resposta",
  accepted: "Aceita",
  refused: "Recusada",
};

export const PRESENCE_LABELS: Record<BuyerPresence, string> = {
  not_applicable: "Não se aplica",
  in_person: "Presencial",
  internet: "Pela internet",
  telephone: "Por telefone",
  home_delivery: "Entrega em domicílio",
  outside_the_establishment: "Presencial, fora do estabelecimento",
  other: "Outros",
};

export const PAYMENT_LABELS: Record<PaymentForm, string> = {
  cash: "Dinheiro",
  cheque: "Cheque",
  credit_card: "Cartão de crédito",
  debit_card: "Cartão de débito",
  store_credit: "Crédito da loja",
  bank_slip: "Boleto",
  bank_deposit: "Depósito bancário",
  pix: "Pix",
  transfer: "Transferência",
  no_payment: "Sem pagamento",
  other: "Outros",
};

/** What is still to be answered by the authority: the page offers to ask again. */
export function isWaiting(status: FiscalDocumentStatus): boolean {
  return status === "processing" || status === "contingency";
}

/** The number as printed on a document: `1/42`, or a dash while there is none. */
export function numberLabel(series: number | null, number: number | null): string {
  return series === null || number === null ? "—" : `${series}/${number}`;
}
