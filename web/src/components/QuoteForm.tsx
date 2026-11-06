import { useState, type FormEvent } from "react";
import type { Party, PaymentTerm, Product, Quote, QuoteInput } from "../api";
import { isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { LinesEditor, linesProblem, linesToInput, newLine, rowsFromLines, type LineRow } from "./LinesEditor";
import { Notice } from "./Notice";
import { useSheetDirty } from "./Sheet";

/** `days` after an ISO day, written as one. */
function plusDays(iso: string, days: number): string {
  const [year, month, day] = iso.split("-").map(Number);
  return todayIso(new Date(year, month - 1, day + days));
}

/** What a quote may point at, as the page or the sheet loaded it. */
export interface QuoteTargets {
  parties: Party[];
  products: Product[];
  terms: PaymentTerm[];
}

interface QuoteFormProps {
  companyId: string;
  /** The quote being changed, or `null` to make one. */
  quote: Quote | null;
  targets: QuoteTargets;
  pending: boolean;
  error: string | null;
  onSubmit: (input: QuoteInput) => void;
  onCancel: () => void;
}

/** Makes a quote, or changes one. Remount it (a `key`) to start over. */
export function QuoteForm({ companyId, quote, targets, pending, error, onSubmit, onCancel }: QuoteFormProps) {
  const [rows, setRowsState] = useState<LineRow[]>(() => (quote === null ? [newLine()] : rowsFromLines(quote.lines)));
  const [fieldError, setFieldError] = useState<string | null>(null);
  const markDirty = useSheetDirty();
  // The lines editor changes rows with clicks as well as typing: either way something was changed.
  const setRows = (next: LineRow[]) => {
    markDirty();
    setRowsState(next);
  };

  // What is in use, plus what the quote already points at, so changing one whose product was
  // deactivated since still shows it.
  const inQuote = new Set(rows.map((row) => row.product_id));
  const productChoices = targets.products.filter((product) => product.active || inQuote.has(product.id));
  const termChoices = targets.terms.filter((term) => term.active || term.id === quote?.payment_term_id);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (!isIsoDate(field("issued_on")) || !isIsoDate(field("valid_until"))) {
      setFieldError("Informe a data de emissão e a validade.");
      return;
    }
    if (field("valid_until") < field("issued_on")) {
      setFieldError("A validade não pode ser antes da emissão.");
      return;
    }
    const problem = linesProblem(rows);
    if (problem !== null) {
      setFieldError(problem);
      return;
    }
    setFieldError(null);
    onSubmit({
      customer_id: field("customer_id"),
      issued_on: field("issued_on"),
      valid_until: field("valid_until"),
      payment_term_id: field("payment_term_id") || undefined,
      notes: field("notes") || undefined,
      lines: linesToInput(rows),
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="quote-info">
        <h3 id="quote-info">Dados do orçamento</h3>
        <div className="field-grid">
          <label>
            Cliente
            <select name="customer_id" required defaultValue={quote?.customer_id ?? ""}>
              <option value="" disabled>
                Escolha um cliente
              </option>
              {targets.parties.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Emitido em
            <input name="issued_on" type="date" required defaultValue={quote?.issued_on ?? todayIso()} />
          </label>
          <label>
            Válido até
            <input name="valid_until" type="date" required defaultValue={quote?.valid_until ?? plusDays(todayIso(), 15)} />
          </label>
          <label>
            Prazo de pagamento
            <select name="payment_term_id" defaultValue={quote?.payment_term_id ?? ""}>
              <option value="">Não informar</option>
              {termChoices.map((term) => (
                <option key={term.id} value={term.id}>
                  {term.name}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>
      <section className="form-section" aria-labelledby="quote-lines">
        <h3 id="quote-lines">Itens</h3>
        <LinesEditor companyId={companyId} rows={rows} onChange={setRows} products={productChoices} />
      </section>
      <section className="form-section" aria-labelledby="quote-notes">
        <h3 id="quote-notes">Outras informações</h3>
        <label>
          Observações
          <textarea name="notes" rows={3} maxLength={2000} defaultValue={quote?.notes ?? ""} />
        </label>
        {quote?.status === "sent" && (
          <p className="muted">
            Alterar um orçamento enviado o devolve a rascunho e o link que o cliente recebeu deixa de valer.
          </p>
        )}
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={quote === null ? PlusIcon : CheckIcon} pending={pending}>
          {quote === null ? "Criar orçamento" : "Salvar alterações"}
        </Button>
      </div>
    </form>
  );
}
