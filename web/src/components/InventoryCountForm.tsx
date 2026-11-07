import { useState, type FormEvent } from "react";
import type { InventoryCount, InventoryCountInput, Product } from "../api";
import { toInputDecimal } from "../lib/decimal";
import { countTemplate, readCountSheet } from "../lib/countSheet";
import { downloadCsv } from "../lib/csv";
import { isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { isAmbiguousSigned, toSignedDecimal } from "../lib/stock";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";
import { useSheetDirty } from "./Sheet";

/** The most lines a count has (the API's bound). */
const MAX_LINES = 500;

/** A line as it is typed: what was found stays text until the form is sent. */
interface CountRow {
  key: number;
  product_id: string;
  counted: string;
}

let lastKey = 0;

function newRow(product_id = "", counted = ""): CountRow {
  lastKey += 1;
  return { key: lastKey, product_id, counted };
}

interface InventoryCountFormProps {
  /** The products that can have stock (kind product). */
  products: Product[];
  /** The count being changed, or `null` to make one. */
  count: InventoryCount | null;
  /** A new count that starts from a spreadsheet: the lines wait until one is read, or until the person asks to type them. */
  viaSheet?: boolean;
  pending: boolean;
  error: string | null;
  onSubmit: (input: InventoryCountInput) => void;
  onCancel: () => void;
}

/** Makes an inventory count, or changes one that is open. Remount it (a `key`) to start over. */
export function InventoryCountForm({ products, count, viaSheet = false, pending, error, onSubmit, onCancel }: InventoryCountFormProps) {
  const [rows, setRowsState] = useState<CountRow[]>(() =>
    count === null ? [newRow()] : count.lines.map((line) => newRow(line.product_id, toInputDecimal(line.counted))),
  );
  const [fieldError, setFieldError] = useState<string | null>(null);
  // What reading the sheet could not use, by line.
  const [sheetProblems, setSheetProblems] = useState<string[]>([]);
  const [typing, setTyping] = useState(!viaSheet);
  const markDirty = useSheetDirty();
  // Rows are added and removed with clicks as well as typed in: either way something was changed.
  const setRows = (next: CountRow[]) => {
    markDirty();
    setRowsState(next);
  };
  const change = (key: number, patch: Partial<CountRow>) => setRows(rows.map((row) => (row.key === key ? { ...row, ...patch } : row)));

  const inCount = new Set(rows.map((row) => row.product_id));
  // The active products, plus what the count already holds.
  const choices = products.filter((product) => product.active || inCount.has(product.id));
  const missing = choices.filter((product) => !inCount.has(product.id));

  function loadAll() {
    // A row nobody started on makes way for the products that are loaded.
    const kept = rows.filter((row) => row.product_id !== "" || row.counted.trim() !== "");
    setRows([...kept, ...missing.slice(0, Math.max(0, MAX_LINES - kept.length)).map((product) => newRow(product.id))]);
  }

  /** Fills the lines from a CSV the person chose: what the sheet counted replaces what was typed. */
  async function readSheet(file: File | undefined) {
    if (file === undefined) return;
    const { lines, problems } = readCountSheet(await file.text(), products.filter((product) => product.active));
    setSheetProblems(problems);
    if (lines.length > 0) {
      setRows(lines.slice(0, MAX_LINES).map((line) => newRow(line.product_id, line.counted)));
      setTyping(true);
    }
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (!isIsoDate(field("counted_on"))) {
      setFieldError("Informe a data da contagem.");
      return;
    }
    const lines: InventoryCountInput["lines"] = [];
    // A row with nothing counted is a product still to count: it is left out, not sent as zero.
    for (const [index, row] of rows.entries()) {
      if (row.counted.trim() === "") continue;
      if (row.product_id === "") {
        setFieldError(`Escolha o produto do item ${index + 1}.`);
        return;
      }
      if (isAmbiguousSigned(row.counted)) {
        setFieldError(`A quantidade do item ${index + 1} é ambígua: 1.500 pode ser mil e quinhentos ou um e meio. Escreva 1500 ou 1,5.`);
        return;
      }
      const counted = toSignedDecimal(row.counted);
      if (counted === undefined || !/^\d+(\.\d{1,3})?$/.test(counted)) {
        setFieldError(`A quantidade do item ${index + 1} precisa ser zero ou mais, com até 3 casas.`);
        return;
      }
      lines.push({ product_id: row.product_id, counted });
    }
    if (lines.length === 0) {
      setFieldError("Informe a quantidade contada de pelo menos um produto.");
      return;
    }
    setFieldError(null);
    onSubmit({ counted_on: field("counted_on"), notes: field("notes") || undefined, lines });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="count-info">
        <h3 id="count-info">Dados do inventário</h3>
        <div className="field-grid">
          <label>
            Data da contagem
            <input name="counted_on" type="date" required defaultValue={count?.counted_on ?? todayIso()} />
          </label>
          <label>
            Observações
            <input name="notes" maxLength={200} autoComplete="off" defaultValue={count?.notes ?? ""} />
          </label>
        </div>
      </section>
      {count === null && (
        <section className="form-section" aria-labelledby="count-sheet">
          <h3 id="count-sheet">Planilha</h3>
          <p className="muted">
            Baixe o modelo, preencha a coluna Contado de cada produto (deixe em branco o que ainda não foi contado) e escolha o arquivo.
          </p>
          <div className="form-actions">
            <Button
              type="button"
              variant="ghost"
              className="btn-small"
              onClick={() => downloadCsv("modelo-de-inventario.csv", countTemplate(products))}
            >
              Baixar modelo
            </Button>
            <label>
              Arquivo da planilha
              <input
                type="file"
                accept=".csv,text/csv"
                aria-label="Arquivo da planilha de contagem"
                disabled={pending}
                onChange={(event) => void readSheet(event.target.files?.[0])}
              />
            </label>
          </div>
          <Notice
            tone="warning"
            message={
              sheetProblems.length === 0
                ? null
                : `Não foi possível usar ${sheetProblems.length === 1 ? "uma linha" : `${sheetProblems.length} linhas`} da planilha. ${sheetProblems.slice(0, 5).join(" ")}${sheetProblems.length > 5 ? ` E mais ${sheetProblems.length - 5}.` : ""}`
            }
          />
          {!typing && (
            <Button type="button" variant="ghost" className="btn-small" onClick={() => setTyping(true)}>
              Preencher à mão
            </Button>
          )}
        </section>
      )}
      {typing && (
      <section className="form-section" aria-labelledby="count-lines">
        <h3 id="count-lines">Produtos contados</h3>
        <p className="muted">
          Informe o que foi encontrado em cada produto, zero se não há nada. O que ficar em branco não entra no inventário.
        </p>
        <fieldset className="lines">
          <legend>Itens</legend>
          {rows.map((row, index) => (
            <div className="line-row" key={row.key}>
              <label className="span-all">
                Produto
                <select
                  aria-label={`Produto do item ${index + 1}`}
                  value={row.product_id}
                  onChange={(event) => change(row.key, { product_id: event.target.value })}
                >
                  <option value="" disabled>
                    Escolha
                  </option>
                  {choices
                    .filter((product) => product.id === row.product_id || !inCount.has(product.id))
                    .map((product) => (
                      <option key={product.id} value={product.id}>
                        {product.name}
                        {product.sku !== null && ` (${product.sku})`}
                      </option>
                    ))}
                </select>
              </label>
              <label>
                Contado
                <input
                  aria-label={`Quantidade contada do item ${index + 1}`}
                  inputMode="decimal"
                  autoComplete="off"
                  placeholder="0"
                  value={row.counted}
                  onChange={(event) => change(row.key, { counted: event.target.value })}
                />
              </label>
              <Button
                type="button"
                variant="ghost"
                className="btn-small"
                aria-label={`Remover item ${index + 1}`}
                disabled={rows.length === 1}
                onClick={() => setRows(rows.filter((item) => item.key !== row.key))}
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
              disabled={rows.length >= MAX_LINES}
              onClick={() => setRows([...rows, newRow()])}
            >
              Adicionar item
            </Button>
            <Button type="button" variant="ghost" className="btn-small" disabled={missing.length === 0} onClick={loadAll}>
              Carregar todos os produtos
            </Button>
          </div>
        </fieldset>
      </section>
      )}
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={count === null ? PlusIcon : CheckIcon} pending={pending}>
          {count === null ? "Criar inventário" : "Salvar alterações"}
        </Button>
      </div>
    </form>
  );
}
