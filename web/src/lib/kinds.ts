import type { BankAccountKind, PaymentMethodKind } from "../api";
import { msg, type Translate } from "../i18n";
import { inDefaultLanguage } from "./inDefaultLanguage";

// The API sends stable keys; these are what people read.
export const BANK_ACCOUNT_KINDS: { key: BankAccountKind; label: string }[] = [
  { key: "checking", label: msg("Checking account") },
  { key: "savings", label: msg("Savings account") },
  { key: "cash", label: msg("Cash box (cash)") },
  { key: "card", label: msg("Credit card") },
];

export const PAYMENT_METHOD_KINDS: { key: PaymentMethodKind; label: string }[] = [
  { key: "cash", label: msg("Cash") },
  { key: "pix", label: msg("Pix") },
  { key: "bank_slip", label: msg("Boleto") },
  { key: "credit_card", label: msg("Credit card") },
  { key: "debit_card", label: msg("Debit card") },
  { key: "bank_transfer", label: msg("Bank transfer") },
  { key: "check", label: msg("Check") },
  { key: "other", label: msg("Other") },
];

export function bankAccountKindLabel(key: string, t: Translate = inDefaultLanguage): string {
  const label = BANK_ACCOUNT_KINDS.find((kind) => kind.key === key)?.label;
  return label === undefined ? key : t(label);
}

export function paymentMethodKindLabel(key: string, t: Translate = inDefaultLanguage): string {
  const label = PAYMENT_METHOD_KINDS.find((kind) => kind.key === key)?.label;
  return label === undefined ? key : t(label);
}
