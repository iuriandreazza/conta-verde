import { useState, type FormEvent } from "react";
import type { BankAccount, TransferInput } from "../api";
import { isAmbiguousDecimal } from "../lib/decimal";
import { centsToApi, isIsoDate, toCents } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { Button } from "./Button";
import { SendIcon } from "./icons/send";
import { Notice } from "./Notice";

interface TransferFormProps {
  /** The accounts money can move between: the active ones. */
  accounts: BankAccount[];
  pending: boolean;
  error: string | null;
  onSubmit: (input: TransferInput) => void;
  onCancel: () => void;
}

/** Moves money from one account of the company to another; a transfer is never edited, only undone. */
export function TransferForm({ accounts, pending, error, onSubmit, onCancel }: TransferFormProps) {
  const [fieldError, setFieldError] = useState<string | null>(null);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    const typedAmount = field("amount");
    const cents = isAmbiguousDecimal(typedAmount) ? null : toCents(typedAmount);
    if (field("from_account_id") === field("to_account_id")) {
      setFieldError("Escolha duas contas diferentes.");
      return;
    }
    if (cents === null || cents === 0n) {
      setFieldError("Informe o valor em reais e centavos, por exemplo 1.234,50.");
      return;
    }
    if (!isIsoDate(field("occurred_on"))) {
      setFieldError("Informe a data da transferência.");
      return;
    }
    setFieldError(null);
    onSubmit({
      from_account_id: field("from_account_id"),
      to_account_id: field("to_account_id"),
      amount: centsToApi(cents),
      occurred_on: field("occurred_on"),
      ...(field("note") !== "" && { note: field("note") }),
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="transfer-info">
        <h3 id="transfer-info">Dados da transferência</h3>
        <div className="field-grid">
          <label>
            De
            <select name="from_account_id" required defaultValue="">
              <option value="" disabled>
                Escolha a conta de origem
              </option>
              {accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Para
            <select name="to_account_id" required defaultValue="">
              <option value="" disabled>
                Escolha a conta de destino
              </option>
              {accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Valor
            <input name="amount" inputMode="decimal" autoComplete="off" required placeholder="0,00" />
          </label>
          <label>
            Data
            <input name="occurred_on" type="date" required defaultValue={todayIso()} />
          </label>
          <label>
            Observação
            <input name="note" maxLength={200} autoComplete="off" placeholder="Opcional" />
          </label>
        </div>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <Button type="button" variant="ghost" disabled={pending} onClick={onCancel}>
          Voltar
        </Button>
        <Button type="submit" icon={SendIcon} pending={pending}>
          Transferir
        </Button>
      </div>
    </form>
  );
}
