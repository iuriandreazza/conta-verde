import { useState, type FormEvent } from "react";
import type { CommissionRule, CommissionRuleInput, Party, Product } from "../api";
import { isAmbiguousDecimal, toApiDecimal, toInputDecimal } from "../lib/decimal";
import { isIsoDate } from "../lib/entryInstallments";
import { todayIso } from "../lib/settlement";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";

interface CommissionRuleFormProps {
  /** The rule being changed, or `null` to register one. */
  rule: CommissionRule | null;
  parties: Party[];
  products: Product[];
  pending: boolean;
  error: string | null;
  onSubmit: (input: CommissionRuleInput) => void;
  onCancel: () => void;
}

/** Registers a commission rule, or changes one. Remount it (a `key`) to start over. */
export function CommissionRuleForm({ rule, parties, products, pending, error, onSubmit, onCancel }: CommissionRuleFormProps) {
  const [fieldError, setFieldError] = useState<string | null>(null);
  const productChoices = products.filter((product) => product.active || product.id === rule?.product_id);
  // A rule keeps naming a seller who stopped being one; dropping it from the choices would widen the rule to everybody.
  const sellerChoices = parties.filter((party) => party.is_seller || party.id === rule?.seller_id);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    const field = (name: string) => String(form.get(name) ?? "").trim();
    if (isAmbiguousDecimal(field("rate")) || toApiDecimal(field("rate")) === undefined) {
      setFieldError("Informe o percentual, por exemplo 5 ou 7,5.");
      return;
    }
    if (!isIsoDate(field("starts_on"))) {
      setFieldError("Informe o primeiro dia de venda que a regra cobre.");
      return;
    }
    if (field("ends_on") !== "" && (!isIsoDate(field("ends_on")) || field("ends_on") < field("starts_on"))) {
      setFieldError("O último dia precisa ser uma data que não venha antes do primeiro.");
      return;
    }
    setFieldError(null);
    onSubmit({
      seller_id: field("seller_id") || undefined,
      product_id: field("product_id") || undefined,
      rate: toApiDecimal(field("rate")) ?? "",
      starts_on: field("starts_on"),
      ends_on: field("ends_on") || undefined,
      active: form.get("active") === "on",
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="rule-who">
        <h3 id="rule-who">A quem e a quê vale</h3>
        <div className="field-grid">
          <label>
            Vendedor
            <select name="seller_id" defaultValue={rule?.seller_id ?? ""}>
              <option value="">Todos os vendedores</option>
              {sellerChoices.map((party) => (
                <option key={party.id} value={party.id}>
                  {party.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Produto
            <select name="product_id" defaultValue={rule?.product_id ?? ""}>
              <option value="">Todos os produtos</option>
              {productChoices.map((product) => (
                <option key={product.id} value={product.id}>
                  {product.name}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>
      <section className="form-section" aria-labelledby="rule-how">
        <h3 id="rule-how">Percentual e período</h3>
        <div className="field-grid">
          <label>
            Percentual (%)
            <input name="rate" inputMode="decimal" autoComplete="off" required placeholder="5" defaultValue={toInputDecimal(rule?.rate ?? null)} />
          </label>
          <label>
            Vale a partir de
            <input name="starts_on" type="date" required defaultValue={rule?.starts_on ?? todayIso()} />
          </label>
          <label>
            Vale até (opcional)
            <input name="ends_on" type="date" defaultValue={rule?.ends_on ?? ""} />
          </label>
          <label className="inline">
            <input name="active" type="checkbox" defaultChecked={rule?.active ?? true} /> Ativa
          </label>
        </div>
        <p className="muted">
          Vale a regra mais específica: vendedor e produto, depois só o vendedor, depois só o produto, depois a geral.
          O dia que conta é o da venda.
        </p>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={rule === null ? PlusIcon : CheckIcon} pending={pending}>
          {rule === null ? "Cadastrar regra" : "Salvar alterações"}
        </Button>
      </div>
    </form>
  );
}
