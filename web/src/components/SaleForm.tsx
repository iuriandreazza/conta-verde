import { useState, type FormEvent } from "react";
import type { Category, CostCenter, Party, PaymentTerm, Product, Sale, SaleInput } from "../api";
import { useT } from "../i18n";
import { treeRows } from "../lib/categoryTree";
import { isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { LinesEditor, linesProblem, linesToInput, newLine, rowsFromLines, type LineRow } from "./LinesEditor";
import { Notice } from "./Notice";
import { useSheetDirty } from "./Sheet";

/** What an order may point at, as the page or the sheet loaded it. */
export interface SaleTargets {
  parties: Party[];
  products: Product[];
  terms: PaymentTerm[];
  categories: Category[];
  costCenters: CostCenter[];
}

interface SaleFormProps {
  companyId: string;
  /** The order being changed, or `null` to make one. */
  sale: Sale | null;
  targets: SaleTargets;
  pending: boolean;
  error: string | null;
  onSubmit: (input: SaleInput) => void;
  onCancel: () => void;
}

/** Makes an order (a sale not yet completed), or changes one. Remount it (a `key`) to start over. */
export function SaleForm({ companyId, sale, targets, pending, error, onSubmit, onCancel }: SaleFormProps) {
  const t = useT();
  const [rows, setRowsState] = useState<LineRow[]>(() => (sale === null ? [newLine()] : rowsFromLines(sale.lines)));
  const [fieldError, setFieldError] = useState<string | null>(null);
  const markDirty = useSheetDirty();
  // The lines editor changes rows with clicks as well as typing: either way something was changed.
  const setRows = (next: LineRow[]) => {
    markDirty();
    setRowsState(next);
  };

  // What is in use, plus what the order already points at.
  const inSale = new Set(rows.map((row) => row.product_id));
  const productChoices = targets.products.filter((product) => product.active || inSale.has(product.id));
  const termChoices = targets.terms.filter((term) => term.active || term.id === sale?.payment_term_id);
  const categoryRows = treeRows(targets.categories).filter(
    ({ category }) => category.id === sale?.category_id || (category.active && category.dre_group !== "transfers"),
  );
  const costCenterChoices = targets.costCenters.filter((center) => center.id === sale?.cost_center_id || center.active);
  const sellers = targets.parties.filter((party) => party.is_seller);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (!isIsoDate(field("sold_on"))) {
      setFieldError(t("Enter the sale date."));
      return;
    }
    const problem = linesProblem(rows, t);
    if (problem !== null) {
      setFieldError(problem);
      return;
    }
    setFieldError(null);
    onSubmit({
      customer_id: field("customer_id"),
      seller_id: field("seller_id") || undefined,
      sold_on: field("sold_on"),
      payment_term_id: field("payment_term_id") || undefined,
      category_id: field("category_id"),
      cost_center_id: field("cost_center_id") || undefined,
      notes: field("notes") || undefined,
      lines: linesToInput(rows),
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="sale-info">
        <h3 id="sale-info">{t("Order details")}</h3>
        <div className="field-grid">
          <label>
            {t("Customer")}
            <select name="customer_id" required defaultValue={sale?.customer_id ?? ""}>
              <option value="" disabled>
                {t("Choose a customer")}
              </option>
              {targets.parties.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Seller")}
            <select name="seller_id" defaultValue={sale?.seller_id ?? ""}>
              <option value="">{t("None")}</option>
              {sellers.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Sale date")}
            <input name="sold_on" type="date" required defaultValue={sale?.sold_on ?? todayIso()} />
          </label>
        </div>
      </section>
      <section className="form-section" aria-labelledby="sale-billing">
        <h3 id="sale-billing">{t("How to receive")}</h3>
        <div className="field-grid">
          <label>
            {t("Payment term")}
            <select name="payment_term_id" defaultValue={sale?.payment_term_id ?? ""}>
              <option value="">{t("Cash, on the day of the sale")}</option>
              {termChoices.map((term) => (
                <option key={term.id} value={term.id}>
                  {term.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Revenue category")}
            <select name="category_id" required defaultValue={sale?.category_id ?? ""}>
              <option value="" disabled>
                {t("Choose a category")}
              </option>
              {categoryRows.map(({ category, depth }) => (
                <option key={category.id} value={category.id}>
                  {"— ".repeat(depth)}
                  {category.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Cost center")}
            <select name="cost_center_id" defaultValue={sale?.cost_center_id ?? ""}>
              <option value="">{t("None")}</option>
              {costCenterChoices.map((center) => (
                <option key={center.id} value={center.id}>
                  {center.name}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>
      <section className="form-section" aria-labelledby="sale-lines">
        <h3 id="sale-lines">{t("Items")}</h3>
        <LinesEditor companyId={companyId} rows={rows} onChange={setRows} products={productChoices} />
      </section>
      <section className="form-section" aria-labelledby="sale-notes">
        <h3 id="sale-notes">{t("Other information")}</h3>
        <label>
          {t("Notes")}
          <textarea name="notes" rows={3} maxLength={2000} defaultValue={sale?.notes ?? ""} />
        </label>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={sale === null ? PlusIcon : CheckIcon} pending={pending}>
          {sale === null ? t("Create order") : t("Save changes")}
        </Button>
      </div>
    </form>
  );
}
