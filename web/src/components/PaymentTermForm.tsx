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
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";

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
  focusName: boolean;
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
  focusName,
  pending,
  error,
  onSubmit,
  onCancel,
}: PaymentTermFormProps) {
  const nextKey = useRef(0);
  const newRow = (days: string, percent: string): Row => ({ key: nextKey.current++, days, percent });
  const [rows, setRows] = useState<Row[]>(() =>
    term === null
      ? [newRow("0", "100")]
      : term.installments.map((item) => newRow(String(item.days), toInputDecimal(item.percent))),
  );
  const [fieldError, setFieldError] = useState<string | null>(null);

  const percents = rows.map((row) => row.percent);
  const total = sumOfPercents(percents);
  const invalidShare = firstInvalidShare(percents);
  const sumMessage =
    total === null
      ? `Preencha a porcentagem da parcela ${(invalidShare ?? 0) + 1}, por exemplo 33,33.`
      : total === WHOLE_IN_HUNDREDTHS
        ? "Total: 100%."
        : total < WHOLE_IN_HUNDREDTHS
          ? `Faltam ${showHundredths(WHOLE_IN_HUNDREDTHS - total)}% para fechar 100%.`
          : `Passou ${showHundredths(total - WHOLE_IN_HUNDREDTHS)}% de 100%.`;

  function update(key: number, change: Partial<Row>) {
    setFieldError(null);
    setRows((current) => current.map((row) => (row.key === key ? { ...row, ...change } : row)));
  }

  function addRow() {
    setRows((current) => {
      const last = current[current.length - 1];
      const proposed = last && /^\d+$/.test(last.days) ? Number(last.days) + NEXT_DUE_STEP_DAYS : null;
      // A proposal past the limit would only be refused: leave the days for the person to fill.
      const days = proposed !== null && proposed <= MAX_INSTALLMENT_DAYS ? String(proposed) : "";
      return [...current, newRow(days, "")];
    });
  }

  function splitEqually() {
    const shares = splitEvenly(rows.length);
    setRows((current) =>
      current.map((row, index) => ({ ...row, percent: toInputDecimal(shares[index]) })),
    );
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) {
      return;
    }
    if (rows.some((row) => !/^\d+$/.test(row.days.trim()))) {
      setFieldError("Informe os dias de cada parcela, com 0 para à vista.");
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
    <form onSubmit={handleSubmit}>
      <label>
        Nome
        <input
          name="name"
          required
          maxLength={200}
          autoComplete="off"
          placeholder="Por exemplo 30/60/90"
          autoFocus={focusName}
          defaultValue={term?.name}
        />
      </label>

      <fieldset className="installments">
        <legend>Parcelas</legend>
        {rows.map((row, index) => (
          <div className="installment-row" key={row.key}>
            <label>
              Dias
              <input
                inputMode="numeric"
                autoComplete="off"
                aria-label={`Dias da parcela ${index + 1}`}
                placeholder="0 = à vista"
                value={row.days}
                onChange={(event) => update(row.key, { days: event.target.value })}
              />
            </label>
            <label>
              %
              <input
                inputMode="decimal"
                autoComplete="off"
                aria-label={`Porcentagem da parcela ${index + 1}`}
                placeholder="0,00"
                value={row.percent}
                onChange={(event) => update(row.key, { percent: event.target.value })}
              />
            </label>
            <Button
              type="button"
              variant="ghost"
              className="btn-small"
              aria-label={`Remover parcela ${index + 1}`}
              disabled={rows.length === 1}
              onClick={() => setRows((current) => current.filter((item) => item.key !== row.key))}
            >
              Remover
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
            Adicionar parcela
          </Button>
          <Button type="button" variant="ghost" className="btn-small" onClick={splitEqually}>
            Dividir igualmente
          </Button>
        </div>
        <p
          role="status"
          className={total === WHOLE_IN_HUNDREDTHS ? "muted" : "installments-sum installments-off"}
        >
          {sumMessage}
        </p>
      </fieldset>

      <label className="inline">
        <input name="active" type="checkbox" defaultChecked={term?.active ?? true} /> Ativo
      </label>
      <Notice message={fieldError ?? error} />
      <div className="form-actions">
        <Button
          type="submit"
          icon={term === null ? PlusIcon : CheckIcon}
          pending={pending}
          disabled={total !== WHOLE_IN_HUNDREDTHS}
        >
          {term === null ? "Adicionar" : "Salvar alterações"}
        </Button>
        {term !== null && (
          <Button type="button" variant="ghost" disabled={pending} onClick={onCancel}>
            Cancelar
          </Button>
        )}
      </div>
    </form>
  );
}
