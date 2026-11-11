import { useState, type FormEvent } from "react";
import type { Category, CostCenter, Party, PaymentTerm, Product, PurchaseOrder, PurchaseOrderInput } from "../api";
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

/** What a purchase order may point at, as the page or the sheet loaded it. */
export interface PurchaseTargets {
  parties: Party[];
  products: Product[];
  terms: PaymentTerm[];
  categories: Category[];
  costCenters: CostCenter[];
}

interface PurchaseFormProps {
  companyId: string;
  /** The draft being changed, or `null` to make an order. */
  order: PurchaseOrder | null;
  targets: PurchaseTargets;
  pending: boolean;
  error: string | null;
  onSubmit: (input: PurchaseOrderInput) => void;
  onCancel: () => void;
}

/** Makes a purchase order, or changes a draft. Remount it (a `key`) to start over. */
export function PurchaseForm({ companyId, order, targets, pending, error, onSubmit, onCancel }: PurchaseFormProps) {
  const t = useT();
  const [rows, setRowsState] = useState<LineRow[]>(() => (order === null ? [newLine()] : rowsFromLines(order.lines)));
  const [fieldError, setFieldError] = useState<string | null>(null);
  const markDirty = useSheetDirty();
  // The lines editor changes rows with clicks as well as typing: either way something was changed.
  const setRows = (next: LineRow[]) => {
    markDirty();
    setRowsState(next);
  };

  // What is in use, plus what the order already points at.
  const inOrder = new Set(rows.map((row) => row.product_id));
  const productChoices = targets.products.filter((product) => product.active || inOrder.has(product.id));
  const suppliers = targets.parties.filter((party) => party.is_supplier || party.id === order?.supplier_id);
  const termChoices = targets.terms.filter((term) => term.active || term.id === order?.payment_term_id);
  const categoryRows = treeRows(targets.categories).filter(
    ({ category }) => category.id === order?.category_id || (category.active && category.dre_group !== "transfers"),
  );
  const costCenterChoices = targets.costCenters.filter((center) => center.id === order?.cost_center_id || center.active);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (!isIsoDate(field("ordered_on"))) {
      setFieldError(t("Enter the order date."));
      return;
    }
    if (field("expected_on") !== "" && !isIsoDate(field("expected_on"))) {
      setFieldError(t("The expected delivery must be a valid date."));
      return;
    }
    const problem = linesProblem(rows);
    if (problem !== null) {
      setFieldError(problem);
      return;
    }
    setFieldError(null);
    onSubmit({
      supplier_id: field("supplier_id"),
      ordered_on: field("ordered_on"),
      expected_on: field("expected_on") || undefined,
      payment_term_id: field("payment_term_id") || undefined,
      category_id: field("category_id"),
      cost_center_id: field("cost_center_id") || undefined,
      notes: field("notes") || undefined,
      lines: linesToInput(rows),
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="purchase-info">
        <h3 id="purchase-info">{t("Order details")}</h3>
        <div className="field-grid">
          <label>
            {t("Supplier")}
            <select name="supplier_id" required defaultValue={order?.supplier_id ?? ""}>
              <option value="" disabled>
                {t("Choose a supplier")}
              </option>
              {suppliers.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Order date")}
            <input name="ordered_on" type="date" required defaultValue={order?.ordered_on ?? todayIso()} />
          </label>
          <label>
            {t("Expected delivery")}
            <input name="expected_on" type="date" defaultValue={order?.expected_on ?? ""} />
          </label>
        </div>
      </section>
      <section className="form-section" aria-labelledby="purchase-billing">
        <h3 id="purchase-billing">{t("How to pay")}</h3>
        <div className="field-grid">
          <label>
            {t("Payment term")}
            <select name="payment_term_id" defaultValue={order?.payment_term_id ?? ""}>
              <option value="">{t("Cash, on the day of receipt")}</option>
              {termChoices.map((term) => (
                <option key={term.id} value={term.id}>
                  {term.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Expense category")}
            <select name="category_id" required defaultValue={order?.category_id ?? ""}>
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
            <select name="cost_center_id" defaultValue={order?.cost_center_id ?? ""}>
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
      <section className="form-section" aria-labelledby="purchase-lines">
        <h3 id="purchase-lines">{t("Items")}</h3>
        <LinesEditor companyId={companyId} rows={rows} onChange={setRows} products={productChoices} pricing="cost" />
      </section>
      <section className="form-section" aria-labelledby="purchase-notes">
        <h3 id="purchase-notes">{t("Other information")}</h3>
        <label>
          {t("Notes")}
          <textarea name="notes" rows={3} maxLength={2000} defaultValue={order?.notes ?? ""} />
        </label>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={order === null ? PlusIcon : CheckIcon} pending={pending}>
          {order === null ? t("Create order") : t("Save changes")}
        </Button>
      </div>
    </form>
  );
}
