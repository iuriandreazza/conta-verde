import { useState, type FormEvent } from "react";
import { api, errorMessage, type AccountBalance, type OpeningBalance } from "../api";
import { isAmbiguousDecimal, toInputDecimal } from "../lib/decimal";
import { isIsoDate } from "../lib/entryInstallments";
import { toSignedApi } from "../lib/signedAmount";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { Notice } from "./Notice";
import { InfoTip } from "./InfoTip";

interface OpeningBalanceFormProps {
  companyId: string;
  item: AccountBalance;
  /** Called with what was stored, so the balances shown are read again. */
  onSaved: (opening: OpeningBalance) => void;
}

/**
 * What the account held at the start of a day, the balance everything after it is added to (ADR 0031).
 * It may be negative: a card or an account in overdraft starts below zero.
 */
export function OpeningBalanceForm({ companyId, item, onSaved }: OpeningBalanceFormProps) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const typed = String(form.get("balance") ?? "").trim();
    const on = String(form.get("on") ?? "").trim();
    // `1.000` could be a thousand or one: the person says which, as in every amount field.
    const balance = isAmbiguousDecimal(typed.replace(/^-\s*/, "")) ? null : toSignedApi(typed);
    if (balance === null) {
      setError("Informe o saldo em reais e centavos, por exemplo 1.234,50 ou -200,00.");
      return;
    }
    if (!isIsoDate(on)) {
      setError("Informe o dia a que esse saldo se refere.");
      return;
    }
    setSaving(true);
    setError(null);
    setSaved(null);
    try {
      const stored = await api.setOpeningBalance(companyId, item.account.id, { balance, on });
      setSaved("Saldo inicial salvo.");
      onSaved(stored);
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  return (
    <form className="opening-form" onSubmit={(event) => void handleSubmit(event)}>
      <label>
        Saldo no início do dia
        <input
          name="balance"
          inputMode="decimal"
          autoComplete="off"
          required
          placeholder="0,00"
          defaultValue={toInputDecimal(item.opening?.balance ?? null)}
        />
      </label>
      <label>
        Dia
        <input name="on" type="date" required defaultValue={item.opening?.on ?? ""} />
      </label>
      <div className="action-with-tip">
        <Button type="submit" icon={CheckIcon} pending={saving}>
          Salvar saldo inicial
        </Button>
        <InfoTip label="Sobre o saldo inicial" align="start">
          <p>
            O saldo da conta soma o que foi baixado e transferido a partir desse dia. Antes dele, o saldo
            aparece zerado.
          </p>
        </InfoTip>
      </div>
      <Notice message={error} />
      <Notice message={saved} tone="success" />
    </form>
  );
}
