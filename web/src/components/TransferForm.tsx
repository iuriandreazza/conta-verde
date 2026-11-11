import { useState, type FormEvent } from "react";
import type { BankAccount, TransferInput } from "../api";
import { isAmbiguousDecimal } from "../lib/decimal";
import { centsToApi, isIsoDate, toCents } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { Button } from "./Button";
import { BackButton } from "./BackButton";
import { SendIcon } from "./icons/send";
import { Notice } from "./Notice";
import { useT } from "../i18n";

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
  const t = useT();
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
      setFieldError(t("Choose two different accounts."));
      return;
    }
    if (cents === null || cents === 0n) {
      setFieldError(t("Give the amount in reais and cents, for example 1.234,50."));
      return;
    }
    if (!isIsoDate(field("occurred_on"))) {
      setFieldError(t("Give the date of the transfer."));
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
        <h3 id="transfer-info">{t("Transfer details")}</h3>
        <div className="field-grid">
          <label>
            {t("From")}
            <select name="from_account_id" required defaultValue="">
              <option value="" disabled>
                {t("Choose the source account")}
              </option>
              {accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("To")}
            <select name="to_account_id" required defaultValue="">
              <option value="" disabled>
                {t("Choose the destination account")}
              </option>
              {accounts.map((account) => (
                <option key={account.id} value={account.id}>
                  {account.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Amount")}
            <input name="amount" inputMode="decimal" autoComplete="off" required placeholder="0,00" />
          </label>
          <label>
            {t("Date")}
            <input name="occurred_on" type="date" required defaultValue={todayIso()} />
          </label>
          <label>
            {t("Note")}
            <input name="note" maxLength={200} autoComplete="off" placeholder={t("Optional")} />
          </label>
        </div>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={SendIcon} pending={pending}>
          {t("Transfer")}
        </Button>
      </div>
    </form>
  );
}
