import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { sign } from "../src/signature.js";
import { createWebhookHandler, seenIds } from "../src/webhook.js";
import { WooCommerce } from "../src/woo.js";
import { order, wooStandIn } from "./standins.js";

const secret = "whsec_" + "cd".repeat(32);
const config = { storeId: "centro", webhookSecret: secret };
const NOW = 1_800_000_000_000;
let store, handle, logs;

beforeEach(async () => {
  store = await wooStandIn();
  logs = [];
  handle = createWebhookHandler({ woo: new WooCommerce({ url: store.url, consumerKey: store.consumerKey, consumerSecret: store.consumerSecret }), config, seen: seenIds(), now: () => NOW, log: (line) => logs.push(line) });
});
afterEach(() => store.close());

/** A delivery as conta-verde sends it. */
function delivery({ type = "sale.completed", reference = "woocommerce:centro:1001", id = "msg_1", at = NOW / 1000, signWith = secret } = {}) {
  const body = JSON.stringify({ id: `ev_${id}`, type, created_at: "2027-01-15T08:00:00Z", company_id: "x", data: { object: { id: "s1", external_reference: reference, status: "completed" } } });
  return { headers: { "webhook-id": id, "webhook-timestamp": String(at), "webhook-signature": sign(signWith, id, String(at), body) }, body };
}

test("a completed sale of this store completes its order, once, however often it is delivered", async () => {
  store.state.orders.push(order({ id: 1001, status: "processing" }));
  assert.deepEqual(await handle(delivery()), { status: 200 });
  assert.deepEqual(store.state.updates, [{ id: 1001, status: "completed" }]);
  // The same delivery again (a retry that crossed the answer) does nothing.
  assert.deepEqual(await handle(delivery()), { status: 200 });
  assert.equal(store.state.updates.length, 1);
});

test("a cancelled sale cancels its order, and an order already in that state is left alone: the loop ends", async () => {
  store.state.orders.push(order({ id: 1001, status: "processing" }), order({ id: 1002, status: "cancelled" }), order({ id: 1003, status: "completed" }));
  await handle(delivery({ type: "sale.cancelled", id: "m1" }));
  await handle(delivery({ type: "sale.cancelled", reference: "woocommerce:centro:1002", id: "m2" }));
  // An order already shipped in the store is not taken back by a cancellation here.
  await handle(delivery({ type: "sale.cancelled", reference: "woocommerce:centro:1003", id: "m3" }));
  assert.deepEqual(store.state.updates, [{ id: 1001, status: "cancelled" }]);
});

test("what is not this store's is accepted and ignored: another store, a point of sale, another event", async () => {
  store.state.orders.push(order({ id: 1001, status: "processing" }));
  for (const [i, change] of [{ reference: "woocommerce:norte:1001" }, { reference: "pdv-77" }, { reference: null }, { type: "charge.paid" }, { type: "webhook.test" }].entries()) {
    assert.deepEqual(await handle(delivery({ id: `m${i}`, ...change })), { status: 200 });
  }
  assert.deepEqual(store.state.updates, []);
});

test("a delivery that is not genuine is refused before anything is read or done", async () => {
  store.state.orders.push(order({ id: 1001, status: "processing" }));
  assert.deepEqual(await handle(delivery({ signWith: "someone-else" })), { status: 401 });
  assert.deepEqual(await handle(delivery({ at: NOW / 1000 - 301 })), { status: 401 });
  assert.deepEqual(await handle({ headers: {}, body: "{}" }), { status: 401 });
  const tampered = delivery();
  assert.deepEqual(await handle({ ...tampered, body: tampered.body.replace("1001", "1002") }), { status: 401 });
  // Signed but not JSON.
  const text = "not json";
  const headers = { "webhook-id": "m9", "webhook-timestamp": String(NOW / 1000), "webhook-signature": sign(secret, "m9", String(NOW / 1000), text) };
  assert.deepEqual(await handle({ headers, body: text }), { status: 400 });
  assert.deepEqual(store.state.updates, []);
});

test("when the store cannot be reached the delivery is not remembered, so conta-verde sends it again", async () => {
  store.state.orders.push(order({ id: 1001, status: "processing" }));
  store.state.down = true;
  assert.deepEqual(await handle(delivery()), { status: 502 });
  store.state.down = false;
  assert.deepEqual(await handle(delivery()), { status: 200 });
  assert.deepEqual(store.state.updates, [{ id: 1001, status: "completed" }]);
  // The log says what happened to an order and never a secret.
  assert.ok(!logs.join("\n").includes(secret));
});

test("the ids remembered are the most recent ones only", () => {
  const seen = seenIds(3);
  for (const id of ["a", "b", "c", "d"]) seen.add(id);
  assert.deepEqual(["a", "b", "c", "d"].map((id) => seen.has(id)), [false, true, true, true]);
});
