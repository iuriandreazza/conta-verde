import { useRef, useState, type FormEvent } from "react";
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
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";

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
  /** The entry being changed, or `null` to register one. */
  entry: Entry | null;
  targets: EntryTargets;
  focusDescription: boolean;
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
  entry,
  targets,
  focusDescription,
  pending,
  error,
  onSubmit,
  onCancel,
}: EntryFormProps) {
  const nextKey = useRef(0);
  const newRow = (due: string, amount: string): Row => ({ key: nextKey.current++, due, amount });
  const [rows, setRows] = useState<Row[]>(() =>
    entry === null
      ? [newRow("", "")]
      : entry.installments.map((item) => newRow(item.due_date, toInputDecimal(item.amount))),
  );
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

  const total = sumCents(rows.map((row) => row.amount));
  const sumMessage =
    total === null
      ? "Preencha o valor de cada parcela, por exemplo 100,50."
      : `Total: ${formatMoney(centsToApi(total))}`;

  function update(key: number, change: Partial<Row>) {
    setFieldError(null);
    setRows((current) => current.map((row) => (row.key === key ? { ...row, ...change } : row)));
  }

  function generate() {
    const count = Number(generator.count);
    const shares = splitAmount(generator.total, count);
    const dates = monthlyDates(generator.first, count);
    if (shares === null || dates === null || count > MAX_INSTALLMENTS) {
      setFieldError(
        "Para gerar as parcelas, informe o valor total, quantas parcelas (até 120) e o primeiro vencimento.",
      );
      return;
    }
    setFieldError(null);
    setRows(shares.map((share, index) => newRow(dates[index], toInputDecimal(share))));
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) {
      return;
    }
    for (const [index, row] of rows.entries()) {
      if (!isIsoDate(row.due)) {
        setFieldError(`Informe o vencimento da parcela ${index + 1}.`);
        return;
      }
      // `1.234` could be a thousand or a little over one: the person says which.
      if (toCents(row.amount) === null || isAmbiguousDecimal(row.amount)) {
        setFieldError(
          `O valor da parcela ${index + 1} precisa estar em reais e centavos, por exemplo 1.234,50.`,
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
    <form onSubmit={handleSubmit}>
      <label>
        Tipo
        <select name="kind" defaultValue={entry?.kind ?? "receivable"}>
          <option value="receivable">A receber</option>
          <option value="payable">A pagar</option>
        </select>
      </label>
      <label>
        Descrição
        <input
          name="description"
          required
          maxLength={200}
          autoComplete="off"
          autoFocus={focusDescription}
          defaultValue={entry?.description}
        />
      </label>
      <label>
        Categoria
        <select name="category_id" required defaultValue={entry?.category_id ?? ""}>
          <option value="" disabled>
            Escolha uma categoria
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
        Centro de custo
        <select name="cost_center_id" defaultValue={entry?.cost_center_id ?? ""}>
          <option value="">Nenhum</option>
          {costCenters.map((center) => (
            <option key={center.id} value={center.id}>
              {center.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        Cliente ou fornecedor
        <select name="party_id" defaultValue={entry?.party_id ?? ""}>
          <option value="">Nenhum</option>
          {targets.parties.map((party) => (
            <option key={party.id} value={party.id}>
              {party.name}
            </option>
          ))}
        </select>
      </label>
      <label>
        Competência
        <input name="competence_date" type="date" required defaultValue={entry?.competence_date} />
      </label>

      <fieldset className="installments">
        <legend>Parcelas</legend>
        <div className="installment-row">
          <label>
            Valor total
            <input
              inputMode="decimal"
              autoComplete="off"
              placeholder="0,00"
              value={generator.total}
              onChange={(event) => setGenerator({ ...generator, total: event.target.value })}
            />
          </label>
          <label>
            Parcelas
            <input
              inputMode="numeric"
              autoComplete="off"
              value={generator.count}
              onChange={(event) => setGenerator({ ...generator, count: event.target.value })}
            />
          </label>
          <label>
            Primeiro vencimento
            <input
              type="date"
              value={generator.first}
              onChange={(event) => setGenerator({ ...generator, first: event.target.value })}
            />
          </label>
        </div>
        <div className="form-actions">
          <Button type="button" variant="ghost" className="btn-small" onClick={generate}>
            Gerar parcelas
          </Button>
        </div>
        {rows.map((row, index) => (
          <div className="installment-row" key={row.key}>
            <label>
              Vencimento
              <input
                type="date"
                aria-label={`Vencimento da parcela ${index + 1}`}
                value={row.due}
                onChange={(event) => update(row.key, { due: event.target.value })}
              />
            </label>
            <label>
              Valor
              <input
                inputMode="decimal"
                autoComplete="off"
                aria-label={`Valor da parcela ${index + 1}`}
                placeholder="0,00"
                value={row.amount}
                onChange={(event) => update(row.key, { amount: event.target.value })}
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
            onClick={() => setRows((current) => [...current, newRow("", "")])}
          >
            Adicionar parcela
          </Button>
        </div>
        <p role="status" className={total === null ? "installments-sum installments-off" : "muted"}>
          {sumMessage}
        </p>
      </fieldset>

      <Notice message={fieldError ?? error} />
      <div className="form-actions">
        <Button type="submit" icon={entry === null ? PlusIcon : CheckIcon} pending={pending}>
          {entry === null ? "Adicionar" : "Salvar alterações"}
        </Button>
        {entry !== null && (
          <Button type="button" variant="ghost" disabled={pending} onClick={onCancel}>
            Cancelar
          </Button>
        )}
      </div>
    </form>
  );
}
