import { ApiError } from "./contaVerde.js";
import { PAID, WITHDRAWN, orderIdOf, saleOf } from "./mapping.js";

/** The slack read before the cursor on every pass: an order modified while the last pass ran is not missed, and repeating one is safe. */
export const OVERLAP_MS = 10 * 60 * 1000;

/** How long an order that could not be imported is looked at again on every pass, in milliseconds. */
export const RETRY_FOR_MS = 7 * 24 * 60 * 60 * 1000;

/**
 * One pass over the store's orders modified since the cursor. A paid order becomes an open sale (once, by its reference); an order
 * taken back cancels its sale while the sale is open. What cannot be done for an order is reported and never stops the others.
 * Returns what happened and the cursor for the next pass.
 */
export async function syncOrders({ woo, api, config, cursor, pending = {}, log, now = () => Date.now() }) {
  const since = new Date(new Date(cursor).getTime() - OVERLAP_MS).toISOString().slice(0, 19);
  const fetched = await woo.ordersModifiedSince(since);
  // An order that could not be imported (a SKU not registered yet, say) is read again until it can, or for a week: the cursor has
  // moved past it, and fixing the cause in the ERP must be enough.
  const again = [];
  for (const id of Object.keys(pending)) {
    if (!fetched.some((order) => String(order.id) === id)) again.push(await woo.order(id));
  }
  const relevant = [...fetched, ...again].filter((order) => PAID.has(order.status) || WITHDRAWN.has(order.status));
  const stillPending = {};
  const skus = new Set(relevant.filter((order) => PAID.has(order.status)).flatMap((order) => (order.line_items ?? []).map((item) => String(item.sku ?? "").trim().toUpperCase()).filter(Boolean)));
  const productIds = await api.productsBySku([...skus]);
  const report = { made: 0, already: 0, cancelled: 0, problems: 0 };
  let next = cursor;
  for (const order of relevant) {
    const held = () => {
      report.problems++;
      const since = pending[order.id] ?? new Date(now()).toISOString();
      if (now() - new Date(since).getTime() < RETRY_FOR_MS) stillPending[order.id] = since;
    };
    try {
      if (PAID.has(order.status)) {
        const { problem, request } = saleOf(order, { ...config, productIds });
        if (problem) {
          held();
          if (!(order.id in pending)) log(`order ${order.id}: not imported: ${problem}`);
        } else {
          const { status } = await api.makeSale(request);
          report[status === 201 ? "made" : "already"]++;
        }
      } else {
        const sale = await api.saleByReference(`woocommerce:${config.storeId}:${order.id}`);
        if (sale?.status === "open") {
          await api.cancelSale(sale.id);
          report.cancelled++;
        } else if (sale?.status === "completed") {
          report.problems++;
          if (!(order.id in pending)) log(`order ${order.id}: taken back in the store but its sale is already completed: undo it in the ERP`);
        }
      }
    } catch (error) {
      held();
      if (error instanceof ApiError && error.status === 409) log(`order ${order.id}: changed in the store after it was imported: ${error.code}`);
      else log(`order ${order.id}: ${error.message}`);
      // A failure of the API itself (not of this order) is tried again from the same cursor.
      if (!(error instanceof ApiError) || error.status >= 500 || error.status === 429) throw error;
    }
    const modified = order.date_modified_gmt ? new Date(`${order.date_modified_gmt}Z`) : null;
    if (modified && modified > new Date(next)) next = modified.toISOString();
  }
  return { report, cursor: next, pending: stillPending };
}

/** The store order a sale of this store stands for, and what the sale's new state asks of it; null for what is not this store's. */
export function orderUpdateFor({ storeId, type, sale }) {
  const orderId = orderIdOf(storeId, sale?.external_reference);
  if (orderId === null) return null;
  if (type === "sale.completed") return { orderId, from: ["processing", "on-hold", "pending"], status: "completed" };
  if (type === "sale.cancelled") return { orderId, from: ["processing", "on-hold", "pending", "completed"], status: "cancelled" };
  return null;
}
