import { useState, type FormEvent } from "react";
import type { PriceList, PriceListInput, Product } from "../api";
import { isAmbiguousDecimal, toApiDecimal, toInputDecimal } from "../lib/decimal";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { useSheetDirty } from "./Sheet";
import { Notice } from "./Notice";

/** The most prices a list holds (the API's bound). */
const MAX_PRICES = 1000;

/** A price as it is typed: both fields are text until the form is sent. */
interface Row {
  key: number;
  product_id: string;
  price: string;
}

let lastKey = 0;
const newRow = (product_id = "", price = ""): Row => {
  lastKey += 1;
  return { key: lastKey, product_id, price };
};

interface PriceListFormProps {
  /** The list being changed, or `null` to register one. */
  list: PriceList | null;
  products: Product[];
  pending: boolean;
  error: string | null;
  onSubmit: (input: PriceListInput) => void;
  onCancel: () => void;
}

/** Registers a price table, or changes one. Remount it (a `key`) to start over. */
export function PriceListForm({ list, products, pending, error, onSubmit, onCancel }: PriceListFormProps) {
  const [rows, setRows] = useState<Row[]>(() =>
    list === null ? [newRow()] : list.items.map((item) => newRow(item.product_id, toInputDecimal(item.price))),
  );
  const [fieldError, setFieldError] = useState<string | null>(null);
  const markDirty = useSheetDirty();

  const inList = new Set(rows.map((row) => row.product_id));
  const productChoices = products.filter((product) => product.active || inList.has(product.id));
  const missing = products.filter((product) => product.active && product.sale_price !== null && !inList.has(product.id));

  // Adding or removing a row is a click, not typing: the sheet must still know something was changed.
  function editRows(update: (current: Row[]) => Row[]) {
    markDirty();
    setRows(update);
  }

  const change = (key: number, patch: Partial<Row>) =>
    setRows((current) => current.map((row) => (row.key === key ? { ...row, ...patch } : row)));

  function chooseProduct(key: number, productId: string) {
    const product = products.find((item) => item.id === productId);
    // The sale price is a starting point, and an empty field is easier to fill than a wrong one is to notice.
    change(key, { product_id: productId, price: product?.sale_price ? toInputDecimal(product.sale_price) : "" });
  }

  function addAllProducts() {
    editRows((current) => [
      ...current.filter((row) => row.product_id !== "" || row.price !== ""),
      ...missing.map((product) => newRow(product.id, toInputDecimal(product.sale_price))),
    ]);
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const position = rows.findIndex((row) => row.product_id === "");
    if (position !== -1) {
      setFieldError(`Escolha o produto do item ${position + 1}.`);
      return;
    }
    if (new Set(rows.map((row) => row.product_id)).size !== rows.length) {
      setFieldError("Cada produto aparece uma vez só na tabela.");
      return;
    }
    const wrong = rows.findIndex((row) => isAmbiguousDecimal(row.price) || toApiDecimal(row.price) === undefined);
    if (wrong !== -1) {
      setFieldError(`Informe o preço do item ${wrong + 1}, por exemplo 12,50.`);
      return;
    }
    setFieldError(null);
    const form = new FormData(event.currentTarget);
    onSubmit({
      name: String(form.get("name") ?? "").trim(),
      items: rows.map((row) => ({ product_id: row.product_id, price: toApiDecimal(row.price) ?? "" })),
      active: form.get("active") === "on",
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="price-list-info">
        <h3 id="price-list-info">Dados da tabela</h3>
        <div className="field-grid">
          <label>
            Nome
            <input name="name" required maxLength={200} autoComplete="off" placeholder="Por exemplo Atacado" defaultValue={list?.name} />
          </label>
          <label className="inline">
            <input name="active" type="checkbox" defaultChecked={list?.active ?? true} /> Ativa
          </label>
        </div>
      </section>
      <section className="form-section" aria-labelledby="price-list-items">
        <h3 id="price-list-items">Preços</h3>
        <fieldset className="lines" aria-labelledby="price-list-items">
          {rows.map((row, index) => (
            <div className="line-row" key={row.key}>
              <label className="span-all">
                Produto ou serviço
                <select
                  aria-label={`Produto ou serviço do preço ${index + 1}`}
                  value={row.product_id}
                  onChange={(event) => chooseProduct(row.key, event.target.value)}
                >
                  <option value="" disabled>
                    Escolha
                  </option>
                  {productChoices.map((product) => (
                    <option key={product.id} value={product.id} disabled={inList.has(product.id) && product.id !== row.product_id}>
                      {product.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Preço
                <input
                  aria-label={`Preço do item ${index + 1}`}
                  inputMode="decimal"
                  autoComplete="off"
                  placeholder="0,00"
                  value={row.price}
                  onChange={(event) => change(row.key, { price: event.target.value })}
                />
              </label>
              <Button
                type="button"
                variant="ghost"
                className="btn-small"
                aria-label={`Remover o preço ${index + 1}`}
                disabled={rows.length === 1}
                onClick={() => editRows((current) => current.filter((item) => item.key !== row.key))}
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
              disabled={rows.length >= MAX_PRICES}
              onClick={() => editRows((current) => [...current, newRow()])}
            >
              Adicionar produto
            </Button>
            <Button type="button" variant="ghost" className="btn-small" disabled={missing.length === 0} onClick={addAllProducts}>
              Incluir todos os produtos
            </Button>
          </div>
          <p className="muted">Incluir todos traz o preço de venda de cada produto que ainda não está na tabela.</p>
        </fieldset>
      </section>
      <Notice message={fieldError ?? error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={list === null ? PlusIcon : CheckIcon} pending={pending}>
          {list === null ? "Cadastrar tabela" : "Salvar alterações"}
        </Button>
      </div>
    </form>
  );
}
