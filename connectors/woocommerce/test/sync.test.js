import assert from "node:assert/strict";
import { afterEach, beforeEach, test } from "node:test";
import { ContaVerde } from "../src/contaVerde.js";
import { syncOrders } from "../src/sync.js";
import { WooCommerce } from "../src/woo.js";
import { contaVerdeStandIn, order, product, wooStandIn } from "./standins.js";

let store, erp, api, woo, logs;
const config = { storeId: "centro", customerId: "c0000000-0000-4000-8000-000000000001", categoryId: "c0000000-0000-4000-8000-000000000002", paymentTermId: null };
const START = "2026-11-01T00:00:00.000Z";
const waits = [];

beforeEach(async () => {
  store = await wooStandIn();
  erp = await contaVerdeStandIn();
  erp.state.products.push(product("cafe-500", "Café 500g"), product("cafe-500-moido", "Café 500g moído"));
  woo = new WooCommerce({ url: store.url, consumerKey: store.consumerKey, consumerSecret: store.consumerSecret });
  api = new ContaVerde({ baseUrl: `${erp.url}/v1`, apiKey: erp.key, sleep: async (ms) => waits.push(ms) });
  logs = [];
  waits.length = 0;
});
afterEach(async () => {
  await store.close();
  await erp.close();
});

const pass = (extra = {}) => syncOrders({ woo, api, config, cursor: START, log: (line) => logs.push(line), ...extra });

test("a paid order becomes one open sale, however many passes see it, and a pending one waits", async () => {
  store.state.orders.push(order({ id: 1, status: "processing" }), order({ id: 2, status: "pending" }), order({ id: 3, status: "completed" }));
  const first = await pass();
  assert.deepEqual(first.report, { made: 2, already: 0, cancelled: 0, problems: 0 });
  assert.deepEqual(erp.state.sales.map((sale) => sale.external_reference).sort(), ["woocommerce:centro:1", "woocommerce:centro:3"]);
  assert.ok(erp.state.sales.every((sale) => sale.status === "open"));
  const second = await pass();
  assert.deepEqual(second.report, { made: 0, already: 2, cancelled: 0, problems: 0 });
  assert.equal(erp.state.sales.length, 2);
  assert.equal(first.cursor, "2026-11-05T13:00:00.000Z");
  // The SKU is matched exactly: the other product that also contains it is not the one.
  assert.equal(erp.state.sales[0].lines[0].product_id, erp.state.products[0].id);
  // It speaks only the public API.
  assert.ok(erp.state.calls.every((call) => /^(GET|POST) \/v1\//.test(call)), erp.state.calls.join("\n"));
});

test("an order taken back cancels its sale while it is open, and says so when the sale is already completed", async () => {
  store.state.orders.push(order({ id: 1 }), order({ id: 2 }));
  await pass();
  erp.state.sales.find((s) => s.external_reference.endsWith(":2")).status = "completed";
  store.state.orders.forEach((o) => {
    o.status = "cancelled";
    o.date_modified_gmt = "2026-11-06T09:00:00";
  });
  const { report } = await pass();
  assert.deepEqual(report, { made: 0, already: 0, cancelled: 1, problems: 1 });
  assert.deepEqual(erp.state.sales.map((s) => s.status).sort(), ["cancelled", "completed"]);
  assert.match(logs.join("\n"), /order 2: taken back in the store but its sale is already completed/);
  // An order that was never imported and is cancelled makes nothing.
  store.state.orders.push(order({ id: 9, status: "cancelled", date_modified_gmt: "2026-11-07T00:00:00" }));
  assert.equal((await pass()).report.cancelled, 0);
});

test("an order the store changed after it was imported is told apart and not looped on", async () => {
  store.state.orders.push(order({ id: 1 }));
  await pass();
  store.state.orders[0].line_items[0].quantity = 5;
  store.state.orders[0].line_items[0].subtotal = "50.50";
  store.state.orders[0].line_items[0].total = "50.50";
  store.state.orders[0].date_modified_gmt = "2026-11-06T09:00:00";
  const { report } = await pass();
  assert.equal(report.problems, 1);
  assert.match(logs.join("\n"), /changed in the store after it was imported: conflict/);
  assert.equal(erp.state.sales.length, 1);
});

test("an order that cannot be imported yet is read again until the cause is fixed, and then it is", async () => {
  store.state.orders.push(order({ id: 1, line_items: [{ id: 1, name: "Chá", sku: "cha-100", quantity: 1, subtotal: "8.00", total: "8.00" }] }));
  const first = await pass();
  assert.equal(first.report.problems, 1);
  assert.deepEqual(Object.keys(first.pending), ["1"]);
  assert.match(logs.join("\n"), /no product with the SKU CHA-100/);
  // The cursor has moved past it; registering the product is enough.
  erp.state.products.push(product("cha-100", "Chá 100g"));
  logs.length = 0;
  const later = await syncOrders({ woo, api, config, cursor: "2026-11-20T00:00:00.000Z", pending: first.pending, log: (line) => logs.push(line) });
  assert.deepEqual(later.report, { made: 1, already: 0, cancelled: 0, problems: 0 });
  assert.deepEqual(later.pending, {});
  // And it is not logged again while it waits.
  assert.deepEqual(logs, []);
});

test("an order that is still not importable after a week is let go", async () => {
  store.state.orders.push(order({ id: 1, line_items: [{ id: 1, name: "Chá", sku: "cha-100", quantity: 1, subtotal: "8.00", total: "8.00" }] }));
  const first = await pass({ now: () => Date.parse("2026-11-10T00:00:00Z") });
  const week = await syncOrders({ woo, api, config, cursor: "2026-11-20T00:00:00.000Z", pending: first.pending, log: () => {}, now: () => Date.parse("2026-11-18T00:00:01Z") });
  assert.deepEqual(week.pending, {});
});

test("a limit is waited out as Retry-After says, and a failure of the API is tried again from the same cursor", async () => {
  store.state.orders.push(order({ id: 1 }));
  erp.state.rateLimited = 2;
  const { report } = await pass();
  assert.equal(report.made, 1);
  assert.deepEqual(waits, [2000, 2000]);
  store.state.orders.push(order({ id: 2, date_modified_gmt: "2026-11-06T10:00:00" }));
  erp.state.down = true;
  await assert.rejects(pass(), /503/);
  erp.state.down = false;
  assert.equal((await pass()).report.made, 1);
});

test("the store's pages are all read, and the store's keys never reach conta-verde", async () => {
  for (let i = 0; i < 120; i++) store.state.orders.push(order({ id: 5000 + i, date_modified_gmt: `2026-11-05T13:${String(i % 60).padStart(2, "0")}:00` }));
  const { report } = await pass();
  assert.equal(report.made, 120);
  assert.ok(store.state.calls.filter((c) => c.includes("/orders?")).length >= 3);
  assert.ok(!JSON.stringify(erp.state).includes(store.consumerSecret));
  await assert.rejects(new WooCommerce({ url: store.url, consumerKey: "no", consumerSecret: "no" }).ordersModifiedSince("2026-01-01T00:00:00"), /401/);
});
