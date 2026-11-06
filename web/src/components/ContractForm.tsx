import { useState, type FormEvent } from "react";
import type { BillingCycle, Category, Contract, ContractInput, CostCenter, Party, PaymentTerm, Product } from "../api";
import { treeRows } from "../lib/categoryTree";
import { CYCLE_LABELS } from "../lib/contractCycle";
import { isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { LinesEditor, linesProblem, linesToInput, newLine, rowsFromLines, type LineRow } from "./LinesEditor";
import { Notice } from "./Notice";
import { useSheetDirty } from "./Sheet";

/** What a contract may point at, as the page or the sheet loaded it. */
export interface ContractTargets {
  parties: Party[];
  products: Product[];
  terms: PaymentTerm[];
  categories: Category[];
  costCenters: CostCenter[];
}

interface ContractFormProps {
  companyId: string;
  /** The contract being changed, or `null` to register one. */
  contract: Contract | null;
  targets: ContractTargets;
  pending: boolean;
  error: string | null;
  onSubmit: (input: ContractInput) => void;
  onCancel: () => void;
}

/** Registers a contract, or changes one. Remount it (a `key`) to start over. */
export function ContractForm({ companyId, contract, targets, pending, error, onSubmit, onCancel }: ContractFormProps) {
  const [rows, setRowsState] = useState<LineRow[]>(() => (contract === null ? [newLine()] : rowsFromLines(contract.lines)));
  const [fieldError, setFieldError] = useState<string | null>(null);
  const markDirty = useSheetDirty();
  // The lines editor changes rows with clicks as well as typing: either way something was changed.
  const setRows = (next: LineRow[]) => {
    markDirty();
    setRowsState(next);
  };

  const inContract = new Set(rows.map((row) => row.product_id));
  const productChoices = targets.products.filter((product) => product.active || inContract.has(product.id));
  const termChoices = targets.terms.filter((term) => term.active || term.id === contract?.payment_term_id);
  const categoryRows = treeRows(targets.categories).filter(
    ({ category }) => category.id === contract?.category_id || (category.active && category.dre_group !== "transfers"),
  );
  const costCenterChoices = targets.costCenters.filter((center) => center.id === contract?.cost_center_id || center.active);
  const sellers = targets.parties.filter((party) => party.is_seller);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (!isIsoDate(field("starts_on"))) {
      setFieldError("Informe a data da primeira cobrança.");
      return;
    }
    if (field("ends_on") !== "" && (!isIsoDate(field("ends_on")) || field("ends_on") < field("starts_on"))) {
      setFieldError("A última cobrança precisa ser uma data que não venha antes da primeira.");
      return;
    }
    const problem = linesProblem(rows);
    if (problem !== null) {
      setFieldError(problem);
      return;
    }
    setFieldError(null);
    onSubmit({
      name: field("name"),
      customer_id: field("customer_id"),
      seller_id: field("seller_id") || undefined,
      payment_term_id: field("payment_term_id") || undefined,
      category_id: field("category_id"),
      cost_center_id: field("cost_center_id") || undefined,
      notes: field("notes") || undefined,
      lines: linesToInput(rows),
      cycle: field("cycle") as BillingCycle,
      starts_on: field("starts_on"),
      ends_on: field("ends_on") || undefined,
      active: form.get("active") === "on",
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="contract-info">
        <h3 id="contract-info">Dados do contrato</h3>
        <div className="field-grid">
          <label>
            Nome
            <input name="name" required maxLength={200} autoComplete="off" defaultValue={contract?.name} />
          </label>
          <label>
            Cliente
            <select name="customer_id" required defaultValue={contract?.customer_id ?? ""}>
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
            Vendedor
            <select name="seller_id" defaultValue={contract?.seller_id ?? ""}>
              <option value="">Nenhum</option>
              {sellers.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Ciclo de cobrança
            <select name="cycle" defaultValue={contract?.cycle ?? "monthly"}>
              {Object.entries(CYCLE_LABELS).map(([key, label]) => (
                <option key={key} value={key}>
                  {label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Primeira cobrança
            <input name="starts_on" type="date" required defaultValue={contract?.starts_on ?? todayIso()} />
          </label>
          <label>
            Última cobrança (opcional)
            <input name="ends_on" type="date" defaultValue={contract?.ends_on ?? ""} />
          </label>
        </div>
      </section>
      <section className="form-section" aria-labelledby="contract-billing">
        <h3 id="contract-billing">Como cobrar</h3>
        <div className="field-grid">
          <label>
            Prazo de pagamento
            <select name="payment_term_id" defaultValue={contract?.payment_term_id ?? ""}>
              <option value="">À vista, no dia da cobrança</option>
              {termChoices.map((term) => (
                <option key={term.id} value={term.id}>
                  {term.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Categoria da receita
            <select name="category_id" required defaultValue={contract?.category_id ?? ""}>
              <option value="" disabled>
                Escolha uma categoria
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
            Centro de custo
            <select name="cost_center_id" defaultValue={contract?.cost_center_id ?? ""}>
              <option value="">Nenhum</option>
              {costCenterChoices.map((center) => (
                <option key={center.id} value={center.id}>
                  {center.name}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>
      <section className="form-section" aria-labelledby="contract-lines">
        <h3 id="contract-lines">Itens</h3>
        <LinesEditor companyId={companyId} rows={rows} onChange={setRows} products={productChoices} />
      </section>
      <section className="form-section" aria-labelledby="contract-notes">
        <h3 id="contract-notes">Observações</h3>
        <label>
          Observações
          <textarea name="notes" rows={3} maxLength={2000} defaultValue={contract?.notes ?? ""} />
        </label>
        <label className="inline">
          <input name="active" type="checkbox" defaultChecked={contract ? contract.status === "active" : true} /> Ativo
        </label>
        <p className="muted">Mudar o contrato vale para as próximas cobranças; o que já foi cobrado não muda.</p>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={contract === null ? PlusIcon : CheckIcon} pending={pending}>
          {contract === null ? "Cadastrar contrato" : "Salvar alterações"}
        </Button>
      </div>
    </form>
  );
}
