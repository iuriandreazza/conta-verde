import { useEffect, useState } from "react";
import { api, type DocumentLine, type LineInput, type PriceList, type Product } from "../api";
import { formatMoney, toApiDecimal, toInputDecimal } from "../lib/decimal";
import { centsToApi } from "../lib/entryInstallments";
import { readUpTo } from "../lib/readUpTo";
import { linesTotalCents, lineTotalCents } from "../lib/saleLines";
import { Button } from "./Button";
import { PlusIcon } from "./icons/plus";

/** The most lines a document has (the API's bound). */
export const MAX_LINES = 100;

/** A line as it is typed: every figure is text until the form is sent. */
export interface LineRow {
  key: number;
  product_id: string;
  description: string;
  quantity: string;
  unit_price: string;
  discount: string;
}

let lastKey = 0;

export function newLine(): LineRow {
  lastKey += 1;
  return { key: lastKey, product_id: "", description: "", quantity: "1", unit_price: "", discount: "" };
}

export function rowsFromLines(lines: DocumentLine[]): LineRow[] {
  return lines.map((line) => ({
    ...newLine(),
    product_id: line.product_id,
    description: line.description,
    quantity: toInputDecimal(line.quantity),
    unit_price: toInputDecimal(line.unit_price),
    discount: line.discount === "0" ? "" : toInputDecimal(line.discount),
  }));
}

const typed = (row: LineRow) => ({
  quantity: row.quantity,
  unitPrice: row.unit_price,
  discount: row.discount,
});

/** Why the lines cannot be sent, or `null` when they can. */
export function linesProblem(rows: LineRow[]): string | null {
  const position = rows.findIndex((row) => row.product_id === "" || row.description.trim() === "");
  if (position !== -1) return `Escolha o produto ou serviço do item ${position + 1}.`;
  const invalid = rows.findIndex((row) => lineTotalCents(typed(row)) === null);
  if (invalid !== -1) {
    return `O item ${invalid + 1} precisa de quantidade (até 3 casas), preço (até 6 casas) e desconto em reais e centavos, que não passe do valor do item.`;
  }
  return null;
}

export function linesToInput(rows: LineRow[]): LineInput[] {
  return rows.map((row) => ({
    product_id: row.product_id,
    description: row.description.trim(),
    quantity: toApiDecimal(row.quantity) ?? "",
    unit_price: toApiDecimal(row.unit_price) ?? "",
    discount: toApiDecimal(row.discount),
  }));
}

interface LinesEditorProps {
  companyId: string;
  rows: LineRow[];
  onChange: (rows: LineRow[]) => void;
  /** What can be chosen: the active products and services, plus whatever the lines already hold. */
  products: Product[];
  /**
   * What fills the price of a chosen product: its sale price, through the price lists (a sale, a quote, a
   * contract), or its cost price (a purchase, which has no price lists).
   */
  pricing?: "sale" | "cost";
}

/**
 * The lines of a quote, a sale or a contract: the product, what is written about it, the quantity,
 * the unit price and a discount in reais. Each line and the total are worked out as the API will, so
 * what is shown is what is stored (ADR 0037).
 */
export function LinesEditor({ companyId, rows, onChange, products, pricing = "sale" }: LinesEditorProps) {
  const costed = pricing === "cost";
  const priceLabel = costed ? "Custo" : "Preço";
  const [priceLists, setPriceLists] = useState<PriceList[]>([]);
  // Kept here and never sent: a document keeps the prices its lines were filled with (ADR 0042).
  const [priceListId, setPriceListId] = useState("");
  useEffect(() => {
    if (costed) return;
    let current = true;
    readUpTo((after) => api.listPriceLists(companyId, { q: "", active: "true", sort: "name" }, after))
      .then((loaded) => current && setPriceLists(loaded))
      // Without the lists the lines are priced by the products, as before.
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [companyId, costed]);
  const priceList = priceLists.find((list) => list.id === priceListId);
  const priceOf = (product: Product | undefined, list: PriceList | undefined): string => {
    const listed = list?.items.find((item) => item.product_id === product?.id);
    const price = listed?.price ?? (costed ? product?.cost_price : product?.sale_price) ?? null;
    return price === null ? "" : toInputDecimal(price);
  };

  function choosePriceList(id: string) {
    setPriceListId(id);
    const list = priceLists.find((candidate) => candidate.id === id);
    // Every line that has a product is priced again: by the list when it names the product, else by the product.
    onChange(
      rows.map((row) =>
        row.product_id === ""
          ? row
          : { ...row, unit_price: priceOf(products.find((item) => item.id === row.product_id), list) },
      ),
    );
  }

  // A line nobody has started on yet is not a mistake: the sum waits for the ones that were chosen.
  const total = linesTotalCents(rows.filter((row) => row.product_id !== "").map(typed));
  const change = (key: number, patch: Partial<LineRow>) =>
    onChange(rows.map((row) => (row.key === key ? { ...row, ...patch } : row)));

  function choose(key: number, productId: string) {
    const product = products.find((item) => item.id === productId);
    // The name and the price are a starting point: they are written on the line and can be changed.
    change(key, {
      product_id: productId,
      description: product?.name ?? "",
      unit_price: priceOf(product, priceList),
    });
  }

  return (
    <fieldset className="lines">
      <legend>Itens</legend>
      {priceLists.length > 0 && (
        <>
          <label className="span-all">
            Tabela de preços
            <select value={priceListId} onChange={(event) => choosePriceList(event.target.value)}>
              <option value="">Preço de venda de cada produto</option>
              {priceLists.map((list) => (
                <option key={list.id} value={list.id}>
                  {list.name}
                </option>
              ))}
            </select>
          </label>
          <p className="muted">Trocar a tabela refaz o preço dos itens que já têm produto.</p>
        </>
      )}
      {rows.map((row, index) => {
        const cents = lineTotalCents(typed(row));
        return (
          <div className="line-row" key={row.key}>
            <label className="span-all">
              Produto ou serviço
              <select
                aria-label={`Produto ou serviço do item ${index + 1}`}
                value={row.product_id}
                onChange={(event) => choose(row.key, event.target.value)}
              >
                <option value="" disabled>
                  Escolha
                </option>
                {products.map((product) => (
                  <option key={product.id} value={product.id}>
                    {product.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="span-all">
              Descrição
              <input
                aria-label={`Descrição do item ${index + 1}`}
                maxLength={200}
                autoComplete="off"
                value={row.description}
                onChange={(event) => change(row.key, { description: event.target.value })}
              />
            </label>
            <label>
              Quantidade
              <input
                aria-label={`Quantidade do item ${index + 1}`}
                inputMode="decimal"
                autoComplete="off"
                value={row.quantity}
                onChange={(event) => change(row.key, { quantity: event.target.value })}
              />
            </label>
            <label>
              {priceLabel}
              <input
                aria-label={`${priceLabel} do item ${index + 1}`}
                inputMode="decimal"
                autoComplete="off"
                placeholder="0,00"
                value={row.unit_price}
                onChange={(event) => change(row.key, { unit_price: event.target.value })}
              />
            </label>
            <label>
              Desconto
              <input
                aria-label={`Desconto do item ${index + 1}`}
                inputMode="decimal"
                autoComplete="off"
                placeholder="0,00"
                value={row.discount}
                onChange={(event) => change(row.key, { discount: event.target.value })}
              />
            </label>
            <div className="line-total">
              <span className="muted">Valor</span>
              <strong>{cents === null ? "—" : formatMoney(centsToApi(cents))}</strong>
            </div>
            <Button
              type="button"
              variant="ghost"
              className="btn-small"
              aria-label={`Remover item ${index + 1}`}
              disabled={rows.length === 1}
              onClick={() => onChange(rows.filter((item) => item.key !== row.key))}
            >
              Remover
            </Button>
          </div>
        );
      })}
      <div className="form-actions">
        <Button
          type="button"
          variant="ghost"
          className="btn-small"
          icon={PlusIcon}
          disabled={rows.length >= MAX_LINES}
          onClick={() => onChange([...rows, newLine()])}
        >
          Adicionar item
        </Button>
      </div>
      <p role="status" className={total === null ? "installments-sum installments-off" : "muted"}>
        {total === null ? "Há itens com valores que não são números válidos." : `Total: ${formatMoney(centsToApi(total))}`}
      </p>
    </fieldset>
  );
}
