import { describe, expect, it } from "vitest";
import type { BankAccount } from "../api";
import { accountDetails } from "./banks";

const account = (fields: Partial<BankAccount>): BankAccount => ({
  id: "a1",
  company_id: "c1",
  kind: "checking",
  name: "Conta principal",
  bank_code: null,
  branch: null,
  number: null,
  active: true,
  ...fields,
});

describe("accountDetails", () => {
  it("names the kind, then the bank, the branch and the number", () => {
    expect(accountDetails(account({ bank_code: "341", branch: "1234", number: "56789X" }))).toBe(
      "Conta corrente · 341 Itaú · ag. 1234 · conta 56789X",
    );
  });

  it("keeps a bank the list does not know as a bare code, never as 'undefined'", () => {
    expect(accountDetails(account({ bank_code: "999", branch: "1" }))).toBe("Conta corrente · 999 · ag. 1");
  });

  it("is only the kind when the account has no bank data", () => {
    expect(accountDetails(account({ kind: "cash" }))).toBe("Caixa (dinheiro)");
  });
});
