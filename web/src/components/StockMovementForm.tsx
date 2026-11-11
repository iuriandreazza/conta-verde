import { useState, type FormEvent } from "react";
import type { Product, StockMovementInput, StockMovementKind } from "../api";
import { msg, useT } from "../i18n";
import { toApiDecimal } from "../lib/decimal";
import { isAmbiguousSigned, toSignedDecimal } from "../lib/stock";
import { isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";
import { useSheetDirty } from "./Sheet";

const KIND_LABELS: Record<StockMovementKind, string> = {
  in: msg("Entry"),
  out: msg("Exit"),
  adjustment: msg("Adjustment"),
};

const QUANTITY_HINTS: Record<StockMovementKind, string> = {
  in: msg("Quantity that came in, up to 3 decimals."),
  out: msg("Quantity that went out, up to 3 decimals."),
  adjustment: msg("Use a sign: positive raises the balance, negative lowers it."),
};

type CostMode = "value" | "unit_cost";

interface StockMovementFormProps {
  /** The products that can have stock: the active ones of kind product. */
  products: Product[];
  pending: boolean;
  error: string | null;
  onSubmit: (input: StockMovementInput) => void;
  onCancel: () => void;
}

/** Writes a movement by hand: an entry (with its cost), an exit or an adjustment of the balance. */
export function StockMovementForm({ products, pending, error, onSubmit, onCancel }: StockMovementFormProps) {
  const t = useT();
  const [kind, setKind] = useState<StockMovementKind>("in");
  const [costMode, setCostMode] = useState<CostMode>("value");
  const [fieldError, setFieldError] = useState<string | null>(null);
  const markDirty = useSheetDirty();

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    const signed = kind === "adjustment";
    if (isAmbiguousSigned(field("quantity")) || isAmbiguousSigned(field("cost"))) {
      setFieldError(t("Use a comma for decimals: 1.500 could be fifteen hundred or one and a half. Write 1500 or 1,5."));
      return;
    }
    const quantity = toSignedDecimal(field("quantity"));
    if (quantity === undefined || !(signed ? /^-?\d+(\.\d{1,3})?$/ : /^\d+(\.\d{1,3})?$/).test(quantity)) {
      setFieldError(
        signed
          ? t("Enter the adjustment quantity, with a sign and up to 3 decimals.")
          : t("Enter the quantity, above zero and with up to 3 decimals."),
      );
      return;
    }
    const cost = toApiDecimal(field("cost"));
    if (kind === "in" && cost === undefined) {
      setFieldError(costMode === "value" ? t("Enter the total cost of the entry.") : t("Enter the unit cost of the entry."));
      return;
    }
    if (!isIsoDate(field("occurred_on"))) {
      setFieldError(t("Enter the movement date."));
      return;
    }
    setFieldError(null);
    onSubmit({
      product_id: field("product_id"),
      kind,
      quantity,
      ...(kind === "in" ? (costMode === "value" ? { value: cost } : { unit_cost: cost }) : {}),
      occurred_on: field("occurred_on"),
      note: field("note") || undefined,
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="stock-info">
        <h3 id="stock-info">{t("What changed")}</h3>
        <fieldset className="segmented">
          <legend>{t("Movement type")}</legend>
          {(Object.keys(KIND_LABELS) as StockMovementKind[]).map((key) => (
            <label key={key}>
              <input type="radio" name="kind" value={key} checked={kind === key} onChange={() => {
                  markDirty();
                  setKind(key);
                }} />
              <span>{t(KIND_LABELS[key])}</span>
            </label>
          ))}
        </fieldset>
        <div className="field-grid">
          <label>
            {t("Product")}
            <select name="product_id" required defaultValue="">
              <option value="" disabled>
                {t("Choose a product")}
              </option>
              {products.map((product) => (
                <option key={product.id} value={product.id}>
                  {product.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Quantity")}
            <input name="quantity" aria-label={t("Quantity")} inputMode="decimal" autoComplete="off" placeholder="0" />
            <span className="muted">{t(QUANTITY_HINTS[kind])}</span>
          </label>
          <label>
            {t("Date")}
            <input name="occurred_on" type="date" required defaultValue={todayIso()} />
          </label>
        </div>
      </section>
      {kind === "in" && (
        <section className="form-section" aria-labelledby="stock-cost">
          <h3 id="stock-cost">{t("Cost of the entry")}</h3>
          <fieldset className="segmented">
            <legend>{t("How to enter the cost")}</legend>
            <label>
              <input type="radio" name="cost_mode" value="value" checked={costMode === "value"} onChange={() => {
                  markDirty();
                  setCostMode("value");
                }} />
              <span>{t("Total cost")}</span>
            </label>
            <label>
              <input type="radio" name="cost_mode" value="unit_cost" checked={costMode === "unit_cost"} onChange={() => {
                  markDirty();
                  setCostMode("unit_cost");
                }} />
              <span>{t("Unit cost")}</span>
            </label>
          </fieldset>
          <label>
            {costMode === "value" ? t("Total cost") : t("Unit cost")}
            <input name="cost" aria-label={costMode === "value" ? t("Total cost value") : t("Unit cost value")} inputMode="decimal" autoComplete="off" placeholder="0,00" />
            <span className="muted">
              {costMode === "value"
                ? t("What the quantity cost, in reais and cents.")
                : t("Up to 6 decimals; the total is rounded to the cent.")}
            </span>
          </label>
        </section>
      )}
      <section className="form-section" aria-labelledby="stock-notes">
        <h3 id="stock-notes">{t("Other information")}</h3>
        <label>
          {t("Note")}
          <input name="note" maxLength={200} autoComplete="off" />
        </label>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={PlusIcon} pending={pending}>
          {t("Record movement")}
        </Button>
      </div>
    </form>
  );
}
