import { useState, type FormEvent } from "react";
import type { CommissionRule, CommissionRuleInput, Party, Product } from "../api";
import { useT } from "../i18n";
import { isAmbiguousDecimal, toApiDecimal, toInputDecimal } from "../lib/decimal";
import { isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";
import { HelpTitle } from "./HelpTitle";

interface CommissionRuleFormProps {
  /** The rule being changed, or `null` to register one. */
  rule: CommissionRule | null;
  parties: Party[];
  products: Product[];
  pending: boolean;
  error: string | null;
  onSubmit: (input: CommissionRuleInput) => void;
  onCancel: () => void;
}

/** Registers a commission rule, or changes one. Remount it (a `key`) to start over. */
export function CommissionRuleForm({ rule, parties, products, pending, error, onSubmit, onCancel }: CommissionRuleFormProps) {
  const t = useT();
  const [fieldError, setFieldError] = useState<string | null>(null);
  const productChoices = products.filter((product) => product.active || product.id === rule?.product_id);
  // A rule keeps naming a seller who stopped being one; dropping it from the choices would widen the rule to everybody.
  const sellerChoices = parties.filter((party) => party.is_seller || party.id === rule?.seller_id);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (isAmbiguousDecimal(field("rate")) || toApiDecimal(field("rate")) === undefined) {
      setFieldError(t("Enter the percentage, for example 5 or 7,5."));
      return;
    }
    if (!isIsoDate(field("starts_on"))) {
      setFieldError(t("Enter the first sale day the rule covers."));
      return;
    }
    if (field("ends_on") !== "" && (!isIsoDate(field("ends_on")) || field("ends_on") < field("starts_on"))) {
      setFieldError(t("The last day must be a date that is not before the first."));
      return;
    }
    setFieldError(null);
    onSubmit({
      seller_id: field("seller_id") || undefined,
      product_id: field("product_id") || undefined,
      rate: toApiDecimal(field("rate")) ?? "",
      starts_on: field("starts_on"),
      ends_on: field("ends_on") || undefined,
      active: form.get("active") === "on",
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="rule-who">
        <HelpTitle id="rule-who" title={t("Who and what it applies to")}>
          <p>
            {t("The most specific rule applies: seller and product, then only the seller, then only the product, then the general one. The day that counts is the sale day.")}
          </p>
        </HelpTitle>
        <div className="field-grid">
          <label>
            {t("Seller")}
            <select name="seller_id" defaultValue={rule?.seller_id ?? ""}>
              <option value="">{t("All sellers")}</option>
              {sellerChoices.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Product")}
            <select name="product_id" defaultValue={rule?.product_id ?? ""}>
              <option value="">{t("All products")}</option>
              {productChoices.map((product) => (
                <option key={product.id} value={product.id}>
                  {product.name}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>
      <section className="form-section" aria-labelledby="rule-how">
        <h3 id="rule-how">{t("Percentage and period")}</h3>
        <div className="field-grid">
          <label>
            {t("Percentage (%)")}
            <input name="rate" inputMode="decimal" autoComplete="off" required placeholder="5" defaultValue={toInputDecimal(rule?.rate ?? null)} />
          </label>
          <label>
            {t("Valid from")}
            <input name="starts_on" type="date" required defaultValue={rule?.starts_on ?? todayIso()} />
          </label>
          <label>
            {t("Valid until (optional)")}
            <input name="ends_on" type="date" defaultValue={rule?.ends_on ?? ""} />
          </label>
          <label className="inline">
            <input name="active" type="checkbox" defaultChecked={rule?.active ?? true} /> {t("Enabled")}
          </label>
        </div>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={rule === null ? PlusIcon : CheckIcon} pending={pending}>
          {rule === null ? t("Register rule") : t("Save changes")}
        </Button>
      </div>
    </form>
  );
}
