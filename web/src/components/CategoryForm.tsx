import { useState, type FormEvent } from "react";
import type { CashFlowGroup, Category, DreGroup, NewCategory } from "../api";
import { treeRows } from "../lib/categoryTree";
import { useT } from "../i18n";
import { CASH_FLOW_GROUPS, DRE_GROUPS, cashFlowLabel, dreLabel } from "../lib/groups";
import { Button } from "./Button";
import { BackButton } from "./BackButton";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";

// Where a group usually lands in the cash flow; the person can still choose another.
const USUAL_CASH_FLOW: Record<DreGroup, CashFlowGroup> = {
  gross_revenue: "operating",
  deductions: "operating",
  costs: "operating",
  operating_expenses: "operating",
  financial_result: "financing",
  investments: "investing",
  outside_result: "financing",
  transfers: "transfer",
};

interface CategoryFormProps {
  /** Every category, to choose the parent from. */
  categories: Category[];
  /** The category being changed, or `null` to create one. */
  editing: Category | null;
  /** The parent to start with when creating a subcategory. */
  initialParentId: string | null;
  pending: boolean;
  error: string | null;
  onCreate: (input: NewCategory) => void;
  onChange: (input: { name: string; active: boolean }) => void;
  onCancel: () => void;
}

/**
 * Creates a category or changes one. A top-level category chooses where it lands in the DRE and the
 * cash flow; a subcategory takes both from its parent, so the form only shows them. Changing one
 * only renames it or switches it on or off: its place and groups are fixed. Remount it (a `key`)
 * to start over.
 */
export function CategoryForm({
  categories,
  editing,
  initialParentId,
  pending,
  error,
  onCreate,
  onChange,
  onCancel,
}: CategoryFormProps) {
  const t = useT();
  const [parentId, setParentId] = useState(initialParentId ?? "");
  const [dreGroup, setDreGroup] = useState<DreGroup>("operating_expenses");
  const [cashFlowGroup, setCashFlowGroup] = useState<CashFlowGroup>("operating");
  const parent = categories.find((category) => category.id === parentId);
  // Under an inactive category, or the one for transfers, or at the last level, nothing can go.
  // In tree order, so that the dashes mean what they look like: a subcategory under its parent.
  const parents = treeRows(categories).filter(
    ({ category }) => category.active && !isTransfer(category) && category.level < 5,
  );

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const name = String(form.get("name") ?? "");
    if (editing !== null) {
      // A disabled checkbox is not in the form data, and the transfer category is always active.
      onChange({ name, active: isTransfer(editing) || form.get("active") === "on" });
    } else if (parent) {
      onCreate({ name, parent_id: parent.id });
    } else {
      onCreate({ name, dre_group: dreGroup, cash_flow_group: cashFlowGroup });
    }
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="category-info">
        <h3 id="category-info">{t("Category details")}</h3>
        <div className="field-grid">
      <label>
        {t("Name")}
        <input
          name="name"
          required
          maxLength={200}
          autoComplete="off"
          defaultValue={editing?.name}
        />
      </label>
      {editing === null ? (
        <>
          <label>
            {t("Subcategory of")}
            <select value={parentId} onChange={(event) => setParentId(event.target.value)}>
              <option value="">{t("None: top-level category")}</option>
              {parents.map(({ category, depth }) => (
                <option key={category.id} value={category.id}>
                  {"— ".repeat(depth)}
                  {category.name}
                </option>
              ))}
            </select>
          </label>
          {parent ? (
            <p className="muted">
              {t("Inherits from {parent}: {dre} in the income statement, {cashFlow} in the cash flow.", {
                parent: parent.name,
                dre: dreLabel(parent.dre_group, t),
                cashFlow: cashFlowLabel(parent.cash_flow_group, t).toLowerCase(),
              })}
            </p>
          ) : (
            <>
              <label>
                {t("DRE group")}
                <select
                  value={dreGroup}
                  onChange={(event) => {
                    const next = event.target.value as DreGroup;
                    setDreGroup(next);
                    setCashFlowGroup(USUAL_CASH_FLOW[next]);
                  }}
                >
                  {DRE_GROUPS.map((group) => (
                    <option key={group.key} value={group.key}>
                      {t(group.label)}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                {t("Cash flow group")}
                <select
                  value={cashFlowGroup}
                  // Moves between accounts are the transfers group in both, nothing else.
                  disabled={dreGroup === "transfers"}
                  onChange={(event) => setCashFlowGroup(event.target.value as CashFlowGroup)}
                >
                  {CASH_FLOW_GROUPS.filter(
                    (group) => group.key !== "transfer" || dreGroup === "transfers",
                  ).map((group) => (
                    <option key={group.key} value={group.key}>
                      {t(group.label)}
                    </option>
                  ))}
                </select>
              </label>
            </>
          )}
        </>
      ) : (
        <>
          <p className="muted">
            {editing.level > 1 ? `${t("Subcategory.")} ` : ""}
            {t("{dre} in the income statement, {cashFlow} in the cash flow. The place in the tree and the groups do not change.", {
              dre: dreLabel(editing.dre_group, t),
              cashFlow: cashFlowLabel(editing.cash_flow_group, t).toLowerCase(),
            })}
          </p>
          <label className="inline">
            <input
              name="active"
              type="checkbox"
              defaultChecked={editing.active}
              disabled={isTransfer(editing)}
            />{" "}
            {t("Active category")}
          </label>
          {isTransfer(editing) && (
            <p className="muted">{t("The transfers category can be renamed but not deactivated.")}</p>
          )}
        </>
      )}
        </div>
      </section>
      <Notice message={error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={editing === null ? PlusIcon : CheckIcon} pending={pending}>
          {editing === null ? t("Add") : t("Save changes")}
        </Button>
      </div>
    </form>
  );
}

/** The category for moves between the company's own accounts: a top-level one in the transfers group. */
export function isTransfer(category: Category): boolean {
  return category.parent_id === null && category.dre_group === "transfers";
}
