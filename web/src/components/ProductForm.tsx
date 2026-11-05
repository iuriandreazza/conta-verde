import { useState, type FormEvent } from "react";
import type { Product, ProductInput, ProductKind } from "../api";
import { isAmbiguousDecimal, toApiDecimal, toInputDecimal } from "../lib/decimal";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice } from "./Notice";

const AMOUNT_FIELDS = [
  { name: "sale_price", label: "Preço de venda" },
  { name: "cost_price", label: "Preço de custo" },
  { name: "minimum_stock", label: "Estoque mínimo" },
];

interface ProductFormProps {
  /** The product being changed, or `null` to register a new one. */
  product: Product | null;
  /** Starts with the cursor in the name, which also brings the form into view. */
  focusName: boolean;
  pending: boolean;
  error: string | null;
  onSubmit: (input: ProductInput) => void;
  onCancel: () => void;
}

/**
 * Registers a product or service, or changes one: the form is the whole product either way, since
 * a change replaces every field. Remount it (a `key`) to start from another product.
 */
export function ProductForm({
  product,
  focusName,
  pending,
  error,
  onSubmit,
  onCancel,
}: ProductFormProps) {
  const [kind, setKind] = useState<ProductKind>(product?.kind ?? "product");
  const isProduct = kind === "product";
  const [fieldError, setFieldError] = useState<string | null>(null);

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const text = (name: string) => String(form.get(name) ?? "");
    const optional = (name: string) => text(name).trim() || undefined;
    const ambiguous = AMOUNT_FIELDS.find(
      ({ name }) => (name !== "minimum_stock" || isProduct) && isAmbiguousDecimal(text(name)),
    );
    if (ambiguous) {
      setFieldError(
        `${ambiguous.label}: "${text(ambiguous.name).trim()}" pode ser lido de dois jeitos. Use vírgula para os decimais (1,234) ou escreva o milhar com decimais (1.234,00).`,
      );
      return;
    }
    setFieldError(null);
    onSubmit({
      kind,
      name: text("name"),
      sku: optional("sku"),
      unit: text("unit"),
      // A service has neither; the fields are disabled, so they are not even read.
      ncm: isProduct ? optional("ncm") : undefined,
      sale_price: toApiDecimal(text("sale_price")),
      cost_price: toApiDecimal(text("cost_price")),
      minimum_stock: isProduct ? toApiDecimal(text("minimum_stock")) : undefined,
      active: form.get("active") === "on",
    });
  }

  return (
    <form className="form-grid" onSubmit={handleSubmit}>
      <label>
        Tipo
        {/* Stock and fiscal data hang from the kind, so it is chosen once. */}
        <select
          value={kind}
          disabled={product !== null}
          onChange={(event) => setKind(event.target.value as ProductKind)}
        >
          <option value="product">Produto</option>
          <option value="service">Serviço</option>
        </select>
      </label>
      <label>
        Nome
        <input
          name="name"
          required
          maxLength={200}
          autoFocus={focusName}
          defaultValue={product?.name}
        />
      </label>
      <label>
        SKU
        <input
          name="sku"
          autoComplete="off"
          spellCheck={false}
          maxLength={60}
          defaultValue={product?.sku ?? ""}
        />
      </label>
      <label>
        Unidade
        <input
          name="unit"
          required
          maxLength={6}
          autoComplete="off"
          placeholder="UN, KG, M2…"
          defaultValue={product?.unit ?? "UN"}
        />
      </label>
      <label>
        NCM
        <input
          name="ncm"
          inputMode="numeric"
          autoComplete="off"
          placeholder="0000.00.00"
          disabled={!isProduct}
          defaultValue={product?.ncm ?? ""}
        />
      </label>
      <label>
        Preço de venda (R$)
        <input
          name="sale_price"
          inputMode="decimal"
          autoComplete="off"
          placeholder="0,00"
          defaultValue={toInputDecimal(product?.sale_price ?? null)}
        />
      </label>
      <label>
        Preço de custo (R$)
        <input
          name="cost_price"
          inputMode="decimal"
          autoComplete="off"
          placeholder="0,00"
          defaultValue={toInputDecimal(product?.cost_price ?? null)}
        />
      </label>
      <label>
        Estoque mínimo
        <input
          name="minimum_stock"
          inputMode="decimal"
          autoComplete="off"
          disabled={!isProduct}
          defaultValue={toInputDecimal(product?.minimum_stock ?? null)}
        />
      </label>
      <label className="inline">
        <input name="active" type="checkbox" defaultChecked={product?.active ?? true} /> Ativo
      </label>
      <Notice message={fieldError ?? error} />
      <div className="form-actions">
        <Button type="submit" icon={product === null ? PlusIcon : CheckIcon} pending={pending}>
          {product === null ? "Adicionar" : "Salvar alterações"}
        </Button>
        {product !== null && (
          <Button type="button" variant="ghost" onClick={onCancel}>
            Cancelar
          </Button>
        )}
      </div>
    </form>
  );
}
