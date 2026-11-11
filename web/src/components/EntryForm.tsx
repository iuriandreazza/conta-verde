import { useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import type { Category, CostCenter, Entry, EntryInput, EntryKind } from "../api";
import { treeRows } from "../lib/categoryTree";
import { formatMoney, isAmbiguousDecimal, toInputDecimal } from "../lib/decimal";
import {
  centsToApi,
  isIsoDate,
  monthlyDates,
  splitAmount,
  sumCents,
  toCents,
} from "../lib/entryInstallments";
import { Button } from "./Button";
import { BackButton } from "./BackButton";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";
import { useT } from "../i18n";
import { useSheetDirty } from "./Sheet";

/** An installment being typed: both fields are text until the form is sent. */
interface Row {
  key: number;
  due: string;
  amount: string;
}

const MAX_INSTALLMENTS = 120;

/** What an entry may point at, as the page loaded it. */
export interface EntryTargets {
  categories: Category[];
  costCenters: CostCenter[];
  parties: { id: string; name: string }[];
}

interface EntryFormProps {
  /** The kind a new entry starts as: a receivable when the person asked for a revenue, a payable for an expense. */
  defaultKind?: EntryKind;
  /** The entry being changed, or `null` to register one. */
  entry: Entry | null;
  targets: EntryTargets;
  pending: boolean;
  error: string | null;
  onSubmit: (input: EntryInput) => void;
  onCancel: () => void;
}

/**
 * Registers an entry, or changes one: the form is the whole entry either way, since a change replaces
 * every installment. It adds the installments up as they are typed, and "Gerar parcelas" fills them
 * in from a total, a number of installments and the first due date, with the remainder on the last
 * (ADR 0003). Remount it (a `key`) to start over.
 */
export function EntryForm({
  defaultKind,
  entry,
  targets,
  pending,
  error,
  onSubmit,
  onCancel,
}: EntryFormProps) {
  const t = useT();
  const nextKey = useRef(0);
  const newRow = (due: string, amount: string): Row => ({ key: nextKey.current++, due, amount });
  const [rows, setRows] = useState<Row[]>(() =>
    entry === null
      ? [newRow("", "")]
      : entry.installments.map((item) => newRow(item.due_date, toInputDecimal(item.amount))),
  );
  const markDirty = useSheetDirty();
  // A row added, removed or generated is a click, not typing: the sheet must still know something was changed.
  const editRows: typeof setRows = (update) => {
    markDirty();
    setRows(update);
  };
  const [fieldError, setFieldError] = useState<string | null>(null);
  const [generator, setGenerator] = useState({ total: "", count: "1", first: "" });

  // What is in use, plus what the entry already points at, so that changing an entry whose category
  // was deactivated since still shows it.
  const categories = treeRows(targets.categories).filter(
    ({ category }) =>
      category.id === entry?.category_id || (category.active && category.dre_group !== "transfers"),
  );
  const costCenters = targets.costCenters.filter(
    (center) => center.id === entry?.cost_center_id || center.active,
  );
  // The choices are the first of the party list: one the entry already points at may be past them,
  // and a form that did not list it would silently remove it on save.
  const parties =
    entry?.party_id && !targets.parties.some((party) => party.id === entry.party_id)
      ? [...targets.parties, { id: entry.party_id, name: t("Current customer or supplier") }]
      : targets.parties;

  const total = sumCents(rows.map((row) => row.amount));
  const sumMessage =
    total === null
      ? t("Fill in the amount of each installment, for example 100,50.")
      : t("Total: {amount}", { amount: formatMoney(centsToApi(total)) });

  function update(key: number, change: Partial<Row>) {
    setFieldError(null);
    setRows((current) => current.map((row) => (row.key === key ? { ...row, ...change } : row)));
  }

  function generate() {
    const count = Number(generator.count);
    // `1.000` could be a thousand or one: the person says which, as in every amount field.
    if (isAmbiguousDecimal(generator.total)) {
      setFieldError(t("The total amount must be in reais and cents, for example 1.000,00."));
      return;
    }
    const shares = splitAmount(generator.total, count);
    const dates = monthlyDates(generator.first, count);
    if (shares === null || dates === null || count > MAX_INSTALLMENTS) {
      setFieldError(
        t("To generate the installments, give the total amount, how many installments (up to 120) and the first due date."),
      );
      return;
    }
    setFieldError(null);
    editRows(shares.map((share, index) => newRow(dates[index], toInputDecimal(share))));
  }

  /** Enter in a field of the generator makes the installments, and never sends the entry. */
  function generateOnEnter(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") {
      event.preventDefault();
      generate();
    }
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) {
      return;
    }
    for (const [index, row] of rows.entries()) {
      if (!isIsoDate(row.due)) {
        setFieldError(t("Give the due date of installment {number}.", { number: index + 1 }));
        return;
      }
      // `1.234` could be a thousand or a little over one: the person says which.
      if (toCents(row.amount) === null || isAmbiguousDecimal(row.amount)) {
        setFieldError(
          t("The amount of installment {number} must be in reais and cents, for example 1.234,50.", { number: index + 1 }),
        );
        return;
      }
    }
    setFieldError(null);
    const form = new FormData(event.currentTarget);
    const optional = (name: string) => String(form.get(name) ?? "") || undefined;
    onSubmit({
      kind: String(form.get("kind")) as EntryKind,
      description: String(form.get("description") ?? ""),
      party_id: optional("party_id"),
      category_id: String(form.get("category_id") ?? ""),
      cost_center_id: optional("cost_center_id"),
      competence_date: String(form.get("competence_date") ?? ""),
      installments: rows.map((row) => ({
        due_date: row.due,
        amount: centsToApi(toCents(row.amount) ?? 0n),
      })),
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="entry-info">
        <h3 id="entry-info">{t("Entry details")}</h3>
        <div className="field-grid">
      <label>
        {t("Type")}
        <select name="kind" defaultValue={entry?.kind ?? defaultKind ?? "receivable"}>
          <option value="receivable">{t("Receivable")}</option>
          <option value="payable">{t("Payable")}</option>
        </select>
      </label>
      <label>
        {t("Description")}
        <input
          name="description"
          required
          maxLength={200}
          autoComplete="off"
          defaultValue={entry?.description}
        />
      </label>
      <label>
        {t("Category")}
        <select name="category_id" required defaultValue={entry?.category_id ?? ""}>
          <option value="" disabled>
            {t("Choose a category")}
          </option>
          {categories.map(({ category, depth }) => (
            <option key={category.id} value={category.id}>
              {"— ".repeat(depth)}
              {category.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("Cost center")}
        <select name="cost_center_id" defaultValue={entry?.cost_center_id ?? ""}>
          <option value="">{t("None")}</option>
          {costCenters.map((center) => (
            <option key={center.id} value={center.id}>
              {center.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("Customer or supplier")}
        <select name="party_id" defaultValue={entry?.party_id ?? ""}>
          <option value="">{t("None")}</option>
          {parties.map((party) => (
            <option key={party.id} value={party.id}>
              {party.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        {t("Accrual date")}
        <input name="competence_date" type="date" required defaultValue={entry?.competence_date} />
      </label>
        </div>
      </section>

      <section className="form-section" aria-labelledby="entry-payment">
        <h3 id="entry-payment">{t("Payment condition")}</h3>
      <fieldset className="installments">
        <legend>{t("Installments")}</legend>
        <div className="installment-row">
          <label>
            {t("Total amount")}
            <input
              inputMode="decimal"
              autoComplete="off"
              placeholder="0,00"
              value={generator.total}
              onChange={(event) => setGenerator({ ...generator, total: event.target.value })}
              onKeyDown={generateOnEnter}
            />
          </label>
          <label>
            {t("Installments")}
            <input
              inputMode="numeric"
              autoComplete="off"
              value={generator.count}
              onChange={(event) => setGenerator({ ...generator, count: event.target.value })}
              onKeyDown={generateOnEnter}
            />
          </label>
          <label>
            {t("First due date")}
            <input
              type="date"
              value={generator.first}
              onChange={(event) => setGenerator({ ...generator, first: event.target.value })}
              onKeyDown={generateOnEnter}
            />
          </label>
        </div>
        <div className="form-actions">
          <Button type="button" variant="ghost" className="btn-small" onClick={generate}>
            {t("Generate installments")}
          </Button>
        </div>
        {rows.map((row, index) => (
          <div className="installment-row" key={row.key}>
            <label>
              {t("Due date")}
              <input
                type="date"
                aria-label={t("Due date of installment {number}", { number: index + 1 })}
                value={row.due}
                onChange={(event) => update(row.key, { due: event.target.value })}
              />
            </label>
            <label>
              {t("Amount")}
              <input
                inputMode="decimal"
                autoComplete="off"
                aria-label={t("Amount of installment {number}", { number: index + 1 })}
                placeholder="0,00"
                value={row.amount}
                onChange={(event) => update(row.key, { amount: event.target.value })}
              />
            </label>
            <Button
              type="button"
              variant="ghost"
              className="btn-small"
              aria-label={t("Remove installment {number}", { number: index + 1 })}
              disabled={rows.length === 1}
              onClick={() => editRows((current) => current.filter((item) => item.key !== row.key))}
            >
              {t("Remove")}
            </Button>
          </div>
        ))}
        <div className="form-actions">
          <Button
            type="button"
            variant="ghost"
            className="btn-small"
            icon={PlusIcon}
            disabled={rows.length >= MAX_INSTALLMENTS}
            onClick={() => editRows((current) => [...current, newRow("", "")])}
          >
            {t("Add installment")}
          </Button>
        </div>
        <p role="status" className={total === null ? "installments-sum installments-off" : "muted"}>
          {sumMessage}
        </p>
      </fieldset>
      </section>

      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={entry === null ? PlusIcon : CheckIcon} pending={pending}>
          {entry === null ? t("Add") : t("Save changes")}
        </Button>
      </div>
    </form>
  );
}
