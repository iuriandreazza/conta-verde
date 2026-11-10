import type {
  BuyerPresence,
  FiscalDocument,
  FiscalDocumentStatus,
  FiscalEventStatus,
  PaymentForm,
  SpecialTaxRegime,
} from "../api";

export const KIND_LABELS: Record<FiscalDocument["kind"], string> = { nfe: "NF-e", nfce: "NFC-e", nfse: "NFS-e" };

export const SPECIAL_REGIME_LABELS: Record<SpecialTaxRegime, string> = {
  none: "Nenhum",
  cooperative_act: "Ato cooperado",
  estimate: "Estimativa",
  municipal_microenterprise: "Microempresa municipal",
  notary_or_registrar: "Notário ou registrador",
  autonomous_professional: "Profissional autônomo",
  professionals_society: "Sociedade de profissionais",
  other: "Outros",
};

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

/**
 * What names a document in a list: an NF-e's series and number; an NFS-e's number once the authority gave it, and its DPS
 * (the number the company gave) while it waits.
 */
export function documentLabel(document: FiscalDocument): string {
  if (document.kind !== "nfse") return numberLabel(document.series, document.number);
  if (document.nfse) return String(document.nfse.number);
  return document.dps ? `DPS ${document.dps.series}/${document.dps.number}` : "—";
}
