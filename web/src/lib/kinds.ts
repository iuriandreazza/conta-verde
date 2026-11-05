import type { BankAccountKind, PaymentMethodKind } from "../api";

// The API sends stable keys; these are what people read.
export const BANK_ACCOUNT_KINDS: { key: BankAccountKind; label: string }[] = [
  { key: "checking", label: "Conta corrente" },
  { key: "savings", label: "Poupança" },
  { key: "cash", label: "Caixa (dinheiro)" },
  { key: "card", label: "Cartão de crédito" },
];

export const PAYMENT_METHOD_KINDS: { key: PaymentMethodKind; label: string }[] = [
  { key: "cash", label: "Dinheiro" },
  { key: "pix", label: "Pix" },
  { key: "bank_slip", label: "Boleto" },
  { key: "credit_card", label: "Cartão de crédito" },
  { key: "debit_card", label: "Cartão de débito" },
  { key: "bank_transfer", label: "Transferência bancária" },
  { key: "check", label: "Cheque" },
  { key: "other", label: "Outro" },
];

export function bankAccountKindLabel(key: string): string {
  return BANK_ACCOUNT_KINDS.find((kind) => kind.key === key)?.label ?? key;
}

export function paymentMethodKindLabel(key: string): string {
  return PAYMENT_METHOD_KINDS.find((kind) => kind.key === key)?.label ?? key;
}
