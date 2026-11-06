import { useState, type FormEvent } from "react";
import type { BankAccount, BankAccountInput, BankAccountKind } from "../api";
import { BANK_ACCOUNT_KINDS } from "../lib/kinds";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";

interface BankAccountFormProps {
  /** The account being changed, or `null` to register one. */
  account: BankAccount | null;
  pending: boolean;
  error: string | null;
  onSubmit: (input: BankAccountInput) => void;
  onCancel: () => void;
}

/**
 * Registers a bank account, a cash box or a card, or changes one. What an account is asked for depends
 * on its kind (cash has no bank, a card only its issuer), so the kind is chosen once and fixed after.
 * Remount it (a `key`) to start over.
 */
export function BankAccountForm({ account, pending, error, onSubmit, onCancel }: BankAccountFormProps) {
  const [chosen, setChosen] = useState<BankAccountKind>("checking");
  const kind = account?.kind ?? chosen;
  const takesBank = kind !== "cash";
  const takesNumber = kind === "checking" || kind === "savings";

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    const text = (name: string) => String(form.get(name) ?? "").trim() || undefined;
    onSubmit({
      kind,
      name: String(form.get("name") ?? ""),
      // A field that is not on the form is not in the form data, so a kind sends only what it takes.
      bank_code: text("bank_code"),
      branch: text("branch"),
      number: text("number"),
      active: form.get("active") === "on",
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="account-info">
        <h3 id="account-info">Dados da conta</h3>
        <div className="field-grid">
          <label>
            Tipo
            <select value={kind} disabled={account !== null} onChange={(event) => setChosen(event.target.value as BankAccountKind)}>
              {BANK_ACCOUNT_KINDS.map((option) => (
                <option key={option.key} value={option.key}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Nome
            <input name="name" required maxLength={200} autoComplete="off" defaultValue={account?.name} />
          </label>
          {takesBank && (
            <label>
              {kind === "card" ? "Banco emissor" : "Código do banco"}
              <input
                name="bank_code"
                inputMode="numeric"
                autoComplete="off"
                maxLength={8}
                placeholder="3 dígitos (ou 8, o ISPB)"
                defaultValue={account?.bank_code ?? ""}
              />
            </label>
          )}
          {takesNumber && (
            <>
              <label>
                Agência
                <input name="branch" autoComplete="off" spellCheck={false} maxLength={14} defaultValue={account?.branch ?? ""} />
              </label>
              <label>
                Conta
                <input
                  name="number"
                  autoComplete="off"
                  spellCheck={false}
                  maxLength={24}
                  placeholder="com o dígito"
                  defaultValue={account?.number ?? ""}
                />
              </label>
            </>
          )}
          <label className="inline">
            <input name="active" type="checkbox" defaultChecked={account?.active ?? true} /> Ativa
          </label>
        </div>
        {!takesBank && <p className="muted">O caixa não tem banco, agência nem número.</p>}
        {takesBank && !takesNumber && (
          <p className="muted">Do cartão guardamos só o banco emissor: o número do cartão nunca é salvo.</p>
        )}
      </section>
      <Notice message={error} />
      <div className="form-footer">
        <Button type="button" variant="ghost" disabled={pending} onClick={onCancel}>
          Voltar
        </Button>
        <Button type="submit" icon={account === null ? PlusIcon : CheckIcon} pending={pending}>
          {account === null ? "Adicionar" : "Salvar alterações"}
        </Button>
      </div>
    </form>
  );
}
