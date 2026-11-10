import { ApiError } from "./contaVerde.js";
import { PAID, WITHDRAWN, orderIdOf, saleOf } from "./mapping.js";

/** The slack read before the cursor on every pass: an order modified while the last pass ran is not missed, and repeating one is safe. */
export const OVERLAP_MS = 10 * 60 * 1000;

/** How long an order that could not be imported is looked at again on every pass, in milliseconds. */
export const RETRY_FOR_MS = 7 * 24 * 60 * 60 * 1000;

/**
 * One pass over the store's orders modified since the cursor. A paid order becomes an open sale (once, by its reference); an order
 * taken back cancels its sale while the sale is open. What cannot be done for an order is reported and never stops the others, and
 * an order already dealt with in the state the store has it in is not asked about again (`handled`: order id to the modification
 * the pass saw), so the ten minutes of overlap cost nothing. Returns what happened and what the next pass starts from.
 */
export async function syncOrders({ woo, api, config, cursor, pending = {}, handled = {}, log, now = () => Date.now() }) {
  const since = new Date(new Date(cursor).getTime() - OVERLAP_MS).toISOString().slice(0, 19);
  const fetched = await woo.ordersModifiedSince(since);
  // An order that could not be imported (a SKU not registered yet, say) is read again until it can, or for a week: the cursor has
  // moved past it, and fixing the cause in the ERP must be enough. One the store no longer has is let go.
  const again = [];
  const stillPending = {};
  for (const id of Object.keys(pending)) {
    if (fetched.some((order) => String(order.id) === id)) continue;
    try {
      again.push(await woo.order(id));
    } catch (error) {
      if (error.status === 404) log(`order ${id}: no longer in the store`);
      else stillPending[id] = pending[id];
    }
  }
  const relevant = [...fetched, ...again].filter((order) => PAID.has(order.status) || WITHDRAWN.has(order.status));
  const skus = new Set(relevant.filter((order) => PAID.has(order.status)).flatMap((order) => (order.line_items ?? []).map((item) => String(item.sku ?? "").trim().toUpperCase()).filter(Boolean)));
  const productIds = await api.productsBySku([...skus]);
  const report = { made: 0, already: 0, cancelled: 0, problems: 0 };
  const nextHandled = Object.fromEntries(Object.entries(handled).filter(([, modified]) => modified >= since));
  let next = cursor;
  for (const order of relevant) {
    const known = order.id in pending;
    if (!known && handled[order.id] === order.date_modified_gmt) continue;
    const held = () => {
      report.problems++;
      const first = pending[order.id] ?? new Date(now()).toISOString();
      if (now() - new Date(first).getTime() < RETRY_FOR_MS) stillPending[order.id] = first;
    };
    try {
      if (PAID.has(order.status)) {
        const { problem, request } = saleOf(order, { ...config, productIds });
        if (problem) {
          held();
          if (!known) log(`order ${order.id}: not imported: ${problem}`);
        } else {
          const { status, body } = await api.makeSale(request);
          report[status === 201 ? "made" : "already"]++;
          if (body.status === "cancelled") log(`order ${order.id}: paid in the store but its sale was cancelled in the ERP`);
          nextHandled[order.id] = order.date_modified_gmt;
        }
      } else {
        const sale = await api.saleByReference(`woocommerce:${config.storeId}:${order.id}`);
        if (sale?.status === "open") {
          await api.cancelSale(sale.id);
          report.cancelled++;
        } else if (sale?.status === "completed") {
          report.problems++;
          log(`order ${order.id}: taken back in the store but its sale is already completed: undo it in the ERP`);
        }
        nextHandled[order.id] = order.date_modified_gmt;
      }
    } catch (error) {
      if (error instanceof ApiError && error.status === 409) {
        // Nothing the person can register or fix makes it the same order again: said once, and not asked about again.
        report.problems++;
        log(`order ${order.id}: changed in the store after it was imported: ${error.code}`);
        nextHandled[order.id] = order.date_modified_gmt;
      } else {
        held();
        if (!known) log(`order ${order.id}: ${error.message}`);
        // A failure of the API itself (not of this order) is tried again from the same cursor.
        if (!(error instanceof ApiError) || error.status >= 500 || error.status === 429) throw error;
      }
    }
    const modified = order.date_modified_gmt ? new Date(`${order.date_modified_gmt}Z`) : null;
    if (modified && modified > new Date(next)) next = modified.toISOString();
  }
  return { report, cursor: next, pending: stillPending, handled: nextHandled };
}

/** The store order a sale of this store stands for, and what the sale's new state asks of it; null for what is not this store's. */
export function orderUpdateFor({ storeId, type, sale }) {
  const orderId = orderIdOf(storeId, sale?.external_reference);
  if (orderId === null) return null;
  if (type === "sale.completed") return { orderId, from: ["processing", "on-hold", "pending"], status: "completed" };
  // A shipped order (`completed` in the store) is never taken back by a cancellation here: that is the person's decision there.
  if (type === "sale.cancelled") return { orderId, from: ["processing", "on-hold", "pending"], status: "cancelled" };
  return null;
}
