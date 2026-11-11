import { useState, type FormEvent } from "react";
import { api, errorMessage, type AccountBalance, type OpeningBalance } from "../api";
import { useT } from "../i18n";
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
  const t = useT();
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
      setError(t("Enter the balance in reais and centavos, for example 1.234,50 or -200,00."));
      return;
    }
    if (!isIsoDate(on)) {
      setError(t("Enter the day this balance refers to."));
      return;
    }
    setSaving(true);
    setError(null);
    setSaved(null);
    try {
      const stored = await api.setOpeningBalance(companyId, item.account.id, { balance, on });
      setSaved(t("Opening balance saved."));
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
        {t("Balance at the start of the day")}
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
        {t("Day")}
        <input name="on" type="date" required defaultValue={item.opening?.on ?? ""} />
      </label>
      <div className="action-with-tip">
        <Button type="submit" icon={CheckIcon} pending={saving}>
          {t("Save opening balance")}
        </Button>
        <InfoTip label={t("About the opening balance")} align="start">
          <p>
            {t("The account balance adds up what was settled and transferred from this day on. Before it, the balance shows as zero.")}
          </p>
        </InfoTip>
      </div>
      <Notice message={error} />
      <Notice message={saved} tone="success" />
    </form>
  );
}
