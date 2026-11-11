import type {
  BuyerPresence,
  FiscalDocument,
  FiscalDocumentStatus,
  FiscalEventStatus,
  PaymentForm,
  SpecialTaxRegime,
} from "../api";
import { msg } from "../i18n";

export const KIND_LABELS: Record<FiscalDocument["kind"], string> = { nfe: "NF-e", nfce: "NFC-e", nfse: "NFS-e" };

// The labels are marks (`msg`): the screen that shows one calls `t` on it.
export const SPECIAL_REGIME_LABELS: Record<SpecialTaxRegime, string> = {
  none: msg("None"),
  cooperative_act: msg("Cooperative act"),
  estimate: msg("Estimate"),
  municipal_microenterprise: msg("Municipal microenterprise"),
  notary_or_registrar: msg("Notary or registrar"),
  autonomous_professional: msg("Autonomous professional"),
  professionals_society: msg("Professionals' society"),
  other: msg("Others"),
};

export const STATUS_LABELS: Record<FiscalDocumentStatus, string> = {
  draft: msg("Draft"),
  processing: msg("Processing"),
  authorized: msg("Authorized"),
  contingency: msg("In contingency"),
  rejected: msg("Rejected"),
  denied: msg("Denied"),
  cancelled: msg("Cancelled"),
};

export const EVENT_STATUS_LABELS: Record<FiscalEventStatus, string> = {
  sent: msg("Sent, no answer"),
  accepted: msg("Accepted"),
  refused: msg("Refused"),
};

export const PRESENCE_LABELS: Record<BuyerPresence, string> = {
  not_applicable: msg("Not applicable"),
  in_person: msg("In person"),
  internet: msg("Over the internet"),
  telephone: msg("By telephone"),
  home_delivery: msg("Home delivery"),
  outside_the_establishment: msg("In person, outside the establishment"),
  other: msg("Others"),
};

export const PAYMENT_LABELS: Record<PaymentForm, string> = {
  cash: msg("Cash"),
  cheque: msg("Cheque"),
  credit_card: msg("Credit card"),
  debit_card: msg("Debit card"),
  store_credit: msg("Store credit"),
  bank_slip: msg("Bank slip (boleto)"),
  bank_deposit: msg("Bank deposit"),
  pix: msg("Pix"),
  transfer: msg("Transfer payment"),
  no_payment: msg("No payment"),
  other: msg("Others"),
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
