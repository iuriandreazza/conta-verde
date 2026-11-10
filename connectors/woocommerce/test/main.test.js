import assert from "node:assert/strict";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { readConfig } from "../src/config.js";
import { start } from "../src/main.js";
import { sign } from "../src/signature.js";
import { contaVerdeStandIn, order, product, wooStandIn } from "./standins.js";

const uuid = (n) => `c0000000-0000-4000-8000-00000000000${n}`;

test("the connector runs: it polls the store, keeps its cursor in a file, and answers conta-verde's webhooks over HTTP", async () => {
  const store = await wooStandIn();
  const erp = await contaVerdeStandIn();
  erp.state.products.push(product("cafe-500"));
  store.state.orders.push(order({ id: 7001 }));
  const dir = mkdtempSync(join(tmpdir(), "connector-"));
  const secret = "whsec_" + "ef".repeat(32);
  const config = readConfig({
    STORE_ID: "centro",
    CONTA_VERDE_URL: `${erp.url}/v1`,
    CONTA_VERDE_API_KEY: erp.key,
    CONTA_VERDE_CUSTOMER_ID: uuid(1),
    CONTA_VERDE_CATEGORY_ID: uuid(2),
    WEBHOOK_SECRET: secret,
    WOO_URL: store.url,
    WOO_CONSUMER_KEY: store.consumerKey,
    WOO_CONSUMER_SECRET: store.consumerSecret,
    PORT: "0",
    POLL_SECONDS: "3600",
    STATE_FILE: join(dir, "state.json"),
    START_FROM: "2026-11-01T00:00:00Z",
  });
  const running = start(config);
  await new Promise((resolve) => running.server.once("listening", resolve));
  const port = running.server.address().port;
  try {
    // The first pass makes the sale and saves the cursor.
    for (let i = 0; i < 50 && erp.state.sales.length === 0; i++) await new Promise((resolve) => setTimeout(resolve, 20));
    assert.equal(erp.state.sales.length, 1);
    for (let i = 0; i < 50; i++) {
      try {
        if (JSON.parse(readFileSync(config.stateFile, "utf8")).cursor.startsWith("2026-11-05T13")) break;
      } catch {}
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    assert.equal(JSON.parse(readFileSync(config.stateFile, "utf8")).cursor, "2026-11-05T13:00:00.000Z");

    assert.equal((await fetch(`http://127.0.0.1:${port}/healthz`)).status, 200);
    assert.equal((await fetch(`http://127.0.0.1:${port}/other`)).status, 404);
    const body = JSON.stringify({ type: "sale.completed", data: { object: { external_reference: "woocommerce:centro:7001" } } });
    const stamp = String(Math.floor(Date.now() / 1000));
    const post = (signature) => fetch(`http://127.0.0.1:${port}/webhooks/conta-verde`, { method: "POST", body, headers: { "webhook-id": "m1", "webhook-timestamp": stamp, "webhook-signature": signature } });
    assert.equal((await post("v1,AAAA")).status, 401);
    assert.equal((await post(sign(secret, "m1", stamp, body))).status, 200);
    assert.deepEqual(store.state.updates, [{ id: 7001, status: "completed" }]);
  } finally {
    running.stop();
    await store.close();
    await erp.close();
  }
});

test("the configuration is read from the environment, refuses what is missing or malformed, and never prints a secret", () => {
  assert.throws(() => readConfig({}), /STORE_ID is not set/);
  const base = { STORE_ID: "centro", CONTA_VERDE_URL: "https://x/api/v1", CONTA_VERDE_API_KEY: "cvk_SECRETKEY", CONTA_VERDE_CUSTOMER_ID: uuid(1), CONTA_VERDE_CATEGORY_ID: uuid(2), WEBHOOK_SECRET: "whsec_SECRET", WOO_URL: "https://loja", WOO_CONSUMER_KEY: "ck", WOO_CONSUMER_SECRET: "cs_SECRET" };
  assert.equal(readConfig(base).storeId, "centro");
  for (const bad of [{ CONTA_VERDE_URL: "https://x/api" }, { STORE_ID: "a b" }, { STORE_ID: "x".repeat(41) }, { CONTA_VERDE_CUSTOMER_ID: "nope" }, { CONTA_VERDE_PAYMENT_TERM_ID: "nope" }, { WEBHOOK_SECRET: "short" }]) {
    try {
      readConfig({ ...base, ...bad });
      assert.fail(`accepted ${JSON.stringify(bad)}`);
    } catch (error) {
      assert.ok(!/SECRET|cvk_/.test(String(error.message)) || /is not set|is the secret/.test(String(error.message)), error.message);
    }
  }
});
