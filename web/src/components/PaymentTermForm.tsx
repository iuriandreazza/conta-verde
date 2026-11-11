import { useRef, useState, type FormEvent } from "react";
import type { PaymentTerm, PaymentTermInput } from "../api";
import { toApiDecimal, toInputDecimal } from "../lib/decimal";
import {
  WHOLE_IN_HUNDREDTHS,
  firstInvalidShare,
  showHundredths,
  splitEvenly,
  sumOfPercents,
} from "../lib/installments";
import { useT } from "../i18n";
import { Button } from "./Button";
import { BackButton } from "./BackButton";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";
import { useSheetDirty } from "./Sheet";

/** An installment being typed: both fields are text until the form is sent. */
interface Row {
  key: number;
  days: string;
  percent: string;
}

const NEXT_DUE_STEP_DAYS = 30;
const MAX_INSTALLMENTS = 24;
const MAX_INSTALLMENT_DAYS = 3650;

interface PaymentTermFormProps {
  /** The term being changed, or `null` to register one. */
  term: PaymentTerm | null;
  pending: boolean;
  error: string | null;
  onSubmit: (input: PaymentTermInput) => void;
  onCancel: () => void;
}

/**
 * Registers a payment term, or changes one: the form is the whole term either way, since a change
 * replaces every installment. It adds up the percentages as they are typed and says what is
 * missing, and "Dividir igualmente" fills them in with the remainder on the last (ADR 0003).
 * Remount it (a `key`) to start over.
 */
export function PaymentTermForm({
  term,
  pending,
  error,
  onSubmit,
  onCancel,
}: PaymentTermFormProps) {
  const t = useT();
  const nextKey = useRef(0);
  const newRow = (days: string, percent: string): Row => ({ key: nextKey.current++, days, percent });
  const [rows, setRows] = useState<Row[]>(() =>
    term === null
      ? [newRow("0", "100")]
      : term.installments.map((item) => newRow(String(item.days), toInputDecimal(item.percent))),
  );
  const markDirty = useSheetDirty();
  // A row added, removed or generated is a click, not typing: the sheet must still know something was changed.
  const editRows: typeof setRows = (update) => {
    markDirty();
    setRows(update);
  };
  const [fieldError, setFieldError] = useState<string | null>(null);

  const percents = rows.map((row) => row.percent);
  const total = sumOfPercents(percents);
  const invalidShare = firstInvalidShare(percents);
  const sumMessage =
    total === null
      ? t("Fill in the percentage of installment {number}, for example 33,33.", { number: (invalidShare ?? 0) + 1 })
      : total === WHOLE_IN_HUNDREDTHS
        ? t("Total: 100%.")
        : total < WHOLE_IN_HUNDREDTHS
          ? t("{percent}% missing to reach 100%.", { percent: showHundredths(WHOLE_IN_HUNDREDTHS - total) })
          : t("{percent}% over 100%.", { percent: showHundredths(total - WHOLE_IN_HUNDREDTHS) });

  function update(key: number, change: Partial<Row>) {
    setFieldError(null);
    setRows((current) => current.map((row) => (row.key === key ? { ...row, ...change } : row)));
  }

  function addRow() {
    editRows((current) => {
      const last = current[current.length - 1];
      const proposed = last && /^\d+$/.test(last.days) ? Number(last.days) + NEXT_DUE_STEP_DAYS : null;
      // A proposal past the limit would only be refused: leave the days for the person to fill.
      const days = proposed !== null && proposed <= MAX_INSTALLMENT_DAYS ? String(proposed) : "";
      return [...current, newRow(days, "")];
    });
  }

  function splitEqually() {
    const shares = splitEvenly(rows.length);
    editRows((current) =>
      current.map((row, index) => ({ ...row, percent: toInputDecimal(shares[index]) })),
    );
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) {
      return;
    }
    if (rows.some((row) => !/^\d+$/.test(row.days.trim()))) {
      setFieldError(t("Enter the days of each installment, with 0 for upfront."));
      return;
    }
    setFieldError(null);
    const form = new FormData(event.currentTarget);
    onSubmit({
      name: String(form.get("name") ?? ""),
      installments: rows.map((row) => ({
        days: Number(row.days.trim()),
        // Not a share the API can read goes as it is, and the API says so.
        percent: toApiDecimal(row.percent) ?? "",
      })),
      active: form.get("active") === "on",
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="term-info">
        <h3 id="term-info">{t("Term details")}</h3>
        <div className="field-grid">
      <label>
        {t("Name")}
        <input
          name="name"
          required
          maxLength={200}
          autoComplete="off"
          placeholder={t("For example 30/60/90")}
          defaultValue={term?.name}
        />
      </label>
      <label className="inline">
        <input name="active" type="checkbox" defaultChecked={term?.active ?? true} /> {t("Active")}
      </label>
        </div>
      </section>

      <section className="form-section" aria-labelledby="term-installments">
        <h3 id="term-installments">{t("Installments of the term")}</h3>
      <fieldset className="installments">
        <legend>{t("Installments")}</legend>
        {rows.map((row, index) => (
          <div className="installment-row" key={row.key}>
            <label>
              {t("Days")}
              <input
                inputMode="numeric"
                autoComplete="off"
                aria-label={t("Days of installment {number}", { number: index + 1 })}
                placeholder={t("0 = upfront")}
                value={row.days}
                onChange={(event) => update(row.key, { days: event.target.value })}
              />
            </label>
            <label>
              %
              <input
                inputMode="decimal"
                autoComplete="off"
                aria-label={t("Percentage of installment {number}", { number: index + 1 })}
                placeholder="0,00"
                value={row.percent}
                onChange={(event) => update(row.key, { percent: event.target.value })}
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
            onClick={addRow}
          >
            {t("Add installment")}
          </Button>
          <Button type="button" variant="ghost" className="btn-small" onClick={splitEqually}>
            {t("Split evenly")}
          </Button>
        </div>
        <p
          role="status"
          className={total === WHOLE_IN_HUNDREDTHS ? "muted" : "installments-sum installments-off"}
        >
          {sumMessage}
        </p>
      </fieldset>
      </section>

      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button
          type="submit"
          icon={term === null ? PlusIcon : CheckIcon}
          pending={pending}
          disabled={total !== WHOLE_IN_HUNDREDTHS}
        >
          {term === null ? t("Add") : t("Save changes")}
        </Button>
      </div>
    </form>
  );
}
