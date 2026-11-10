import { lineOf } from "./money.js";

/** Woo statuses that mean the customer paid: the order becomes an open sale in the ERP. */
export const PAID = new Set(["processing", "completed"]);
/** Woo statuses that take an order back: the sale, if it is still open, is cancelled. */
export const WITHDRAWN = new Set(["cancelled", "refunded", "failed"]);

/** `woocommerce:{store}:{order}`: the store is part of it because two stores of one company both have an order 123. */
export const referenceOf = (storeId, orderId) => `woocommerce:${storeId}:${orderId}`;

/** The order id of a reference this connector made for this store, or null for any other sale (a point of sale, another store). */
export function orderIdOf(storeId, reference) {
  const prefix = `woocommerce:${storeId}:`;
  if (typeof reference !== "string" || !reference.startsWith(prefix)) return null;
  const id = reference.slice(prefix.length);
  return /^\d+$/.test(id) ? Number(id) : null;
}

/**
 * The request that makes the sale of an order, from the order alone: the same order always gives the same request, so sending it
 * again is the same sale and an order the store changed afterwards is told apart by the API (409). Prices are the order's own,
 * never the product's today. Returns `{ request }`, or `{ problem }` naming why the order cannot be a sale.
 *
 * Not imported, said in the README: shipping, fees and taxes (only the items), and a currency other than BRL.
 */
export function saleOf(order, { storeId, customerId, categoryId, paymentTermId, productIds }) {
  if (order.currency && order.currency !== "BRL") return { problem: `the order is in ${order.currency}, not BRL` };
  const items = order.line_items ?? [];
  if (items.length === 0) return { problem: "the order has no items" };
  const lines = [];
  for (const item of items) {
    const sku = String(item.sku ?? "").trim().toUpperCase();
    if (sku === "") return { problem: `item "${item.name}" has no SKU` };
    const productId = productIds.get(sku);
    if (!productId) return { problem: `no product with the SKU ${sku}` };
    try {
      const line = lineOf({ quantity: item.quantity, subtotal: item.subtotal, total: item.total });
      lines.push({ product_id: productId, description: String(item.name ?? sku).slice(0, 200), quantity: String(item.quantity), unit_price: line.unitPrice, discount: line.discount });
    } catch (error) {
      return { problem: `item ${sku}: ${error.message}` };
    }
  }
  const day = String(order.date_created ?? "").slice(0, 10);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day)) return { problem: "the order has no date" };
  return {
    request: {
      external_reference: referenceOf(storeId, order.id),
      customer_id: customerId,
      sold_on: day,
      payment_term_id: paymentTermId ?? null,
      category_id: categoryId,
      notes: `Pedido WooCommerce #${order.number ?? order.id} (${storeId})`,
      lines,
    },
  };
}
