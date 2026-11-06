import type { BankAccount } from "../api";
import { bankAccountKindLabel } from "./kinds";
/**
 * The banks most small businesses in Brazil use, by the code the Central Bank gives them (COMPE), so
 * an account shows who holds it and not only a number. A code that is not here still shows as typed.
 */
const BANKS: Record<string, string> = {
  "001": "Banco do Brasil",
  "033": "Santander",
  "041": "Banrisul",
  "070": "BRB",
  "077": "Inter",
  "104": "Caixa Econômica",
  "208": "BTG Pactual",
  "212": "Original",
  "237": "Bradesco",
  "260": "Nubank",
  "336": "C6 Bank",
  "341": "Itaú",
  "422": "Safra",
  "748": "Sicredi",
  "756": "Sicoob",
};

export function bankName(code: string): string | undefined {
  return BANKS[code];
}

/** The kind, then what the bank calls the account, in one line so the table keeps its columns for figures. */
export function accountDetails(account: BankAccount): string {
  // The code is free text: a bank the list does not know stays a bare code.
  const bank = account.bank_code ? [account.bank_code, bankName(account.bank_code)].filter(Boolean).join(" ") : null;
  const branch = account.branch ? `ag. ${account.branch}` : null;
  const number = account.number ? `conta ${account.number}` : null;
  return [bankAccountKindLabel(account.kind), bank, branch, number].filter(Boolean).join(" · ");
}
