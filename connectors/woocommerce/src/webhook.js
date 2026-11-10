import { orderUpdateFor } from "./sync.js";
import { verify } from "./signature.js";

/**
 * What conta-verde tells this store: a sale of this store that was completed completes its order, and one that was cancelled
 * cancels it. Every delivery is checked (signature, timestamp, a `Webhook-Id` not seen before) before it is read, anything that is
 * not this store's is accepted and ignored, and an order already in the state asked for is left alone: that is what ends the loop
 * when the cancellation came from the store in the first place.
 */
export function createWebhookHandler({ woo, config, seen, now = () => Date.now(), log }) {
  return async function handle({ headers, body }) {
    const id = headers["webhook-id"];
    const ok = verify({
      secret: config.webhookSecret,
      id,
      timestamp: headers["webhook-timestamp"],
      signatures: headers["webhook-signature"],
      body,
      nowSeconds: Math.floor(now() / 1000),
    });
    if (!ok) return { status: 401 };
    if (seen.has(id)) return { status: 200 };
    let event;
    try {
      event = JSON.parse(body);
    } catch {
      return { status: 400 };
    }
    const update = orderUpdateFor({ storeId: config.storeId, type: event.type, sale: event.data?.object });
    if (update) {
      try {
        const order = await woo.order(update.orderId);
        if (update.from.includes(order.status)) {
          await woo.setStatus(update.orderId, update.status);
          log(`order ${update.orderId}: ${order.status} -> ${update.status} (${event.type})`);
        }
      } catch (error) {
        log(`order ${update.orderId}: ${error.message}`);
        // Not remembered: conta-verde tries the delivery again.
        return { status: 502 };
      }
    }
    seen.add(id);
    return { status: 200 };
  };
}

/** The ids already handled, the most recent `limit` of them. */
export function seenIds(limit = 1000) {
  const ids = new Set();
  return {
    has: (id) => ids.has(id),
    add(id) {
      ids.add(id);
      if (ids.size > limit) ids.delete(ids.values().next().value);
    },
  };
}
