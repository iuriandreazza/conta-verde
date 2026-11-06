import { useState, type FormEvent } from "react";
import type { Product, StockMovementInput, StockMovementKind } from "../api";
import { toApiDecimal } from "../lib/decimal";
import { isAmbiguousSigned, toSignedDecimal } from "../lib/stock";
import { isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";
import { useSheetDirty } from "./Sheet";

const KIND_LABELS: Record<StockMovementKind, string> = {
  in: "Entrada",
  out: "Saída",
  adjustment: "Ajuste",
};

const QUANTITY_HINTS: Record<StockMovementKind, string> = {
  in: "Quantidade que entrou, até 3 casas.",
  out: "Quantidade que saiu, até 3 casas.",
  adjustment: "Use sinal: positivo aumenta o saldo, negativo diminui.",
};

type CostMode = "value" | "unit_cost";

interface StockMovementFormProps {
  /** The products that can have stock: the active ones of kind product. */
  products: Product[];
  pending: boolean;
  error: string | null;
  onSubmit: (input: StockMovementInput) => void;
  onCancel: () => void;
}

/** Writes a movement by hand: an entry (with its cost), an exit or an adjustment of the balance. */
export function StockMovementForm({ products, pending, error, onSubmit, onCancel }: StockMovementFormProps) {
  const [kind, setKind] = useState<StockMovementKind>("in");
  const [costMode, setCostMode] = useState<CostMode>("value");
  const [fieldError, setFieldError] = useState<string | null>(null);
  const markDirty = useSheetDirty();

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    const signed = kind === "adjustment";
    if (isAmbiguousSigned(field("quantity")) || isAmbiguousSigned(field("cost"))) {
      setFieldError("Use vírgula nas casas decimais: 1.500 pode ser mil e quinhentos ou um e meio. Escreva 1500 ou 1,5.");
      return;
    }
    const quantity = toSignedDecimal(field("quantity"));
    if (quantity === undefined || !(signed ? /^-?\d+(\.\d{1,3})?$/ : /^\d+(\.\d{1,3})?$/).test(quantity)) {
      setFieldError(
        signed
          ? "Informe a quantidade do ajuste, com sinal e até 3 casas."
          : "Informe a quantidade, maior que zero e com até 3 casas.",
      );
      return;
    }
    const cost = toApiDecimal(field("cost"));
    if (kind === "in" && cost === undefined) {
      setFieldError(costMode === "value" ? "Informe o custo total da entrada." : "Informe o custo unitário da entrada.");
      return;
    }
    if (!isIsoDate(field("occurred_on"))) {
      setFieldError("Informe a data do movimento.");
      return;
    }
    setFieldError(null);
    onSubmit({
      product_id: field("product_id"),
      kind,
      quantity,
      ...(kind === "in" ? (costMode === "value" ? { value: cost } : { unit_cost: cost }) : {}),
      occurred_on: field("occurred_on"),
      note: field("note") || undefined,
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="stock-info">
        <h3 id="stock-info">O que mudou</h3>
        <fieldset className="segmented">
          <legend>Tipo de movimento</legend>
          {(Object.keys(KIND_LABELS) as StockMovementKind[]).map((key) => (
            <label key={key}>
              <input type="radio" name="kind" value={key} checked={kind === key} onChange={() => {
                  markDirty();
                  setKind(key);
                }} />
              <span>{KIND_LABELS[key]}</span>
            </label>
          ))}
        </fieldset>
        <div className="field-grid">
          <label>
            Produto
            <select name="product_id" required defaultValue="">
              <option value="" disabled>
                Escolha um produto
              </option>
              {products.map((product) => (
                <option key={product.id} value={product.id}>
                  {product.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Quantidade
            <input name="quantity" aria-label="Quantidade" inputMode="decimal" autoComplete="off" placeholder="0" />
            <span className="muted">{QUANTITY_HINTS[kind]}</span>
          </label>
          <label>
            Data
            <input name="occurred_on" type="date" required defaultValue={todayIso()} />
          </label>
        </div>
      </section>
      {kind === "in" && (
        <section className="form-section" aria-labelledby="stock-cost">
          <h3 id="stock-cost">Custo da entrada</h3>
          <fieldset className="segmented">
            <legend>Como informar o custo</legend>
            <label>
              <input type="radio" name="cost_mode" value="value" checked={costMode === "value"} onChange={() => {
                  markDirty();
                  setCostMode("value");
                }} />
              <span>Custo total</span>
            </label>
            <label>
              <input type="radio" name="cost_mode" value="unit_cost" checked={costMode === "unit_cost"} onChange={() => {
                  markDirty();
                  setCostMode("unit_cost");
                }} />
              <span>Custo unitário</span>
            </label>
          </fieldset>
          <label>
            {costMode === "value" ? "Custo total" : "Custo unitário"}
            <input name="cost" aria-label={costMode === "value" ? "Valor do custo total" : "Valor do custo unitário"} inputMode="decimal" autoComplete="off" placeholder="0,00" />
            <span className="muted">
              {costMode === "value" ? "O que a quantidade custou, em reais e centavos." : "Até 6 casas decimais; o total é arredondado ao centavo."}
            </span>
          </label>
        </section>
      )}
      <section className="form-section" aria-labelledby="stock-notes">
        <h3 id="stock-notes">Outras informações</h3>
        <label>
          Observação
          <input name="note" maxLength={200} autoComplete="off" />
        </label>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={PlusIcon} pending={pending}>
          Registrar movimento
        </Button>
      </div>
    </form>
  );
}
