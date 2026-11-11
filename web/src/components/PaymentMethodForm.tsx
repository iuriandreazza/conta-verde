import type { FormEvent } from "react";
import type { PaymentMethod, PaymentMethodInput, PaymentMethodKind } from "../api";
import { useT } from "../i18n";
import { PAYMENT_METHOD_KINDS } from "../lib/kinds";
import { Button } from "./Button";
import { BackButton } from "./BackButton";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";

interface PaymentMethodFormProps {
  /** The method being changed, or `null` to register one. */
  method: PaymentMethod | null;
  pending: boolean;
  error: string | null;
  onSubmit: (input: PaymentMethodInput) => void;
  onCancel: () => void;
}

/** Registers a payment method, or changes one. Remount it (a `key`) to start over. */
export function PaymentMethodForm({ method, pending, error, onSubmit, onCancel }: PaymentMethodFormProps) {
  const t = useT();
  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    onSubmit({
      kind: String(form.get("kind")) as PaymentMethodKind,
      name: String(form.get("name") ?? ""),
      active: form.get("active") === "on",
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="method-info">
        <h3 id="method-info">{t("Payment method details")}</h3>
        <div className="field-grid">
          <label>
            {t("Type")}
            <select name="kind" defaultValue={method?.kind ?? "pix"}>
              {PAYMENT_METHOD_KINDS.map((option) => (
                <option key={option.key} value={option.key}>
                  {t(option.label)}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Name")}
            <input
              name="name"
              required
              maxLength={200}
              autoComplete="off"
              placeholder={t("How it shows in sales, for example Visa card")}
              defaultValue={method?.name}
            />
          </label>
          <label className="inline">
            <input name="active" type="checkbox" defaultChecked={method?.active ?? true} /> {t("Active payment method")}
          </label>
        </div>
      </section>
      <Notice message={error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={method === null ? PlusIcon : CheckIcon} pending={pending}>
          {method === null ? t("Add") : t("Save changes")}
        </Button>
      </div>
    </form>
  );
}
