import { useState, type FormEvent } from "react";
import type { Category, CostCenter, EntryKind, Recurrence, RecurrenceFrequency, RecurrenceInput } from "../api";
import { treeRows } from "../lib/categoryTree";
import { isAmbiguousDecimal, toInputDecimal } from "../lib/decimal";
import { centsToApi, isIsoDate, toCents } from "../lib/entryInstallments";
import { FREQUENCIES } from "../lib/recurrence";
import { Button } from "./Button";
import { BackButton } from "./BackButton";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";
import { HelpTitle } from "./HelpTitle";
import { useT } from "../i18n";

/** What a recurrence may point at, as the page or the sheet loaded it. */
export interface RecurrenceTargets {
  categories: Category[];
  costCenters: CostCenter[];
  parties: { id: string; name: string }[];
  names: Map<string, string>;
}

interface RecurrenceFormProps {
  /** The recurrence being changed, or `null` to register one. */
  recurrence: Recurrence | null;
  targets: RecurrenceTargets;
  pending: boolean;
  error: string | null;
  onSubmit: (input: RecurrenceInput) => void;
  onCancel: () => void;
}

/** Registers a recurrence, or changes one. Remount it (a `key`) to start over. */
export function RecurrenceForm({ recurrence, targets, pending, error, onSubmit, onCancel }: RecurrenceFormProps) {
  const t = useT();
  const [fieldError, setFieldError] = useState<string | null>(null);
  const { categories, costCenters, parties, names } = targets;

  // What is in use, plus what the recurrence already points at, so that changing one whose category was
  // deactivated since still shows it.
  const categoryRows = treeRows(categories).filter(
    ({ category }) =>
      category.id === recurrence?.category_id || (category.active && category.dre_group !== "transfers"),
  );
  const costCenterChoices = costCenters.filter((center) => center.id === recurrence?.cost_center_id || center.active);
  const partyChoices =
    recurrence?.party_id && !parties.some((party) => party.id === recurrence.party_id)
      ? [...parties, { id: recurrence.party_id, name: names.get(recurrence.party_id) ?? recurrence.party_id }]
      : parties;

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    const typedAmount = field("amount");
    // `1.000` could be a thousand or one: the person says which, as in every amount field.
    const cents = isAmbiguousDecimal(typedAmount) ? null : toCents(typedAmount);
    if (cents === null || cents === 0n) {
      setFieldError(t("Give the amount in reais and cents, for example 1.234,50."));
      return;
    }
    if (!isIsoDate(field("starts_on"))) {
      setFieldError(t("Give the first due date."));
      return;
    }
    if (field("ends_on") !== "" && !isIsoDate(field("ends_on"))) {
      setFieldError(t("The last day is not a date."));
      return;
    }
    if (field("ends_on") !== "" && field("ends_on") < field("starts_on")) {
      setFieldError(t("The last day cannot be before the first due date."));
      return;
    }
    setFieldError(null);
    onSubmit({
      kind: field("kind") as EntryKind,
      description: field("description"),
      party_id: field("party_id") || undefined,
      category_id: field("category_id"),
      cost_center_id: field("cost_center_id") || undefined,
      amount: centsToApi(cents),
      frequency: field("frequency") as RecurrenceFrequency,
      starts_on: field("starts_on"),
      ends_on: field("ends_on") || undefined,
      active: form.get("active") === "on",
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="recurrence-info">
        <h3 id="recurrence-info">{t("Recurrence details")}</h3>
        <div className="field-grid">
          <label>
            {t("Type")}
            <select name="kind" defaultValue={recurrence?.kind ?? "payable"}>
              <option value="payable">{t("Payable")}</option>
              <option value="receivable">{t("Receivable")}</option>
            </select>
          </label>
          <label>
            {t("Description")}
            <input name="description" required maxLength={200} autoComplete="off" defaultValue={recurrence?.description} />
          </label>
          <label>
            {t("Category")}
            <select name="category_id" required defaultValue={recurrence?.category_id ?? ""}>
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
            <select name="cost_center_id" defaultValue={recurrence?.cost_center_id ?? ""}>
              <option value="">{t("None")}</option>
              {costCenterChoices.map((center) => (
                <option key={center.id} value={center.id}>
                  {center.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("Customer or supplier")}
            <select name="party_id" defaultValue={recurrence?.party_id ?? ""}>
              <option value="">{t("None")}</option>
              {partyChoices.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.name}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>
      <section className="form-section" aria-labelledby="recurrence-when">
        <HelpTitle id="recurrence-when" title={t("Amount and frequency")}>
          <p>{t("Changing the recurrence applies to the next entries; the ones already generated do not change.")}</p>
        </HelpTitle>
        <div className="field-grid">
          <label>
            {t("Amount each time")}
            <input
              name="amount"
              inputMode="decimal"
              autoComplete="off"
              required
              placeholder="0,00"
              defaultValue={recurrence ? toInputDecimal(recurrence.amount) : ""}
            />
          </label>
          <label>
            {t("Frequency")}
            <select name="frequency" defaultValue={recurrence?.frequency ?? "monthly"}>
              {FREQUENCIES.map((option) => (
                <option key={option.key} value={option.key}>
                  {t(option.label)}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("First due date")}
            <input name="starts_on" type="date" required defaultValue={recurrence?.starts_on ?? ""} />
          </label>
          <label>
            {t("Last day (optional)")}
            <input name="ends_on" type="date" defaultValue={recurrence?.ends_on ?? ""} />
          </label>
          <label className="inline">
            <input name="active" type="checkbox" defaultChecked={recurrence?.active ?? true} /> {t("Running")}
          </label>
        </div>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={recurrence === null ? PlusIcon : CheckIcon} pending={pending}>
          {recurrence === null ? t("Add") : t("Save changes")}
        </Button>
      </div>
    </form>
  );
}
