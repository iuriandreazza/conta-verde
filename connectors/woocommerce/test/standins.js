// Local stand-ins for the two services the connector speaks to. They are written from the public documentation of each and hold
// what the connector relies on; neither is the real service (see the README's "What was not validated").
import { createServer } from "node:http";
import { randomUUID } from "node:crypto";

async function listen(handler) {
  const server = createServer(async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const url = new URL(request.url, "http://stand-in");
    const body = chunks.length ? JSON.parse(Buffer.concat(chunks).toString("utf8")) : undefined;
    const reply = await handler({ method: request.method, url, headers: request.headers, body });
    response.writeHead(reply.status, { "content-type": "application/json", ...(reply.headers ?? {}) });
    response.end(reply.body === undefined ? "" : JSON.stringify(reply.body));
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  return { url: `http://127.0.0.1:${server.address().port}`, close: () => new Promise((resolve) => server.close(resolve)) };
}

/** The `v1` API of conta-verde: products, sales by reference, cancel; a key; and a `429` on demand. */
export async function contaVerdeStandIn({ key = "cvk_test" } = {}) {
  const state = { products: [], sales: [], calls: [], rateLimited: 0, down: false };
  const server = await listen(({ method, url, headers, body }) => {
    state.calls.push(`${method} ${url.pathname}${url.search}`);
    if (headers.authorization !== `Bearer ${key}`) return { status: 401, body: { code: "unauthenticated", title: "x", status: 401, detail: "no", type: "urn:x" } };
    if (state.rateLimited > 0) {
      state.rateLimited--;
      return { status: 429, headers: { "retry-after": "2" }, body: { code: "rate_limited", title: "x", status: 429, detail: "slow down", type: "urn:x" } };
    }
    if (state.down) return { status: 503, body: { code: "internal", title: "x", status: 503, detail: "down", type: "urn:x" } };
    if (method === "GET" && url.pathname === "/v1/products") {
      const q = (url.searchParams.get("q") ?? "").toLowerCase();
      const items = state.products.filter((p) => p.name.toLowerCase().includes(q) || (p.sku ?? "").toLowerCase().includes(q));
      return { status: 200, body: { items, next_cursor: null } };
    }
    if (method === "GET" && url.pathname === "/v1/sales") {
      const found = state.sales.filter((s) => s.external_reference === url.searchParams.get("external_reference"));
      return { status: 200, body: { items: found, next_cursor: null } };
    }
    if (method === "POST" && url.pathname === "/v1/sales") {
      const held = state.sales.find((s) => s.external_reference === body.external_reference);
      if (held) {
        const same = JSON.stringify(held.request) === JSON.stringify(body);
        return same ? { status: 200, body: held } : { status: 409, body: { code: "conflict", title: "x", status: 409, detail: "reference used with another request", type: "urn:x" } };
      }
      const sale = { id: randomUUID(), status: "open", external_reference: body.external_reference, request: body, lines: body.lines };
      state.sales.push(sale);
      return { status: 201, body: sale };
    }
    const cancel = /^\/v1\/sales\/([^/]+)\/cancel$/.exec(url.pathname);
    if (method === "POST" && cancel) {
      const sale = state.sales.find((s) => s.id === cancel[1]);
      if (!sale) return { status: 404, body: { code: "not_found", title: "x", status: 404, detail: "no", type: "urn:x" } };
      if (sale.status === "completed") return { status: 409, body: { code: "conflict", title: "x", status: 409, detail: "completed", type: "urn:x" } };
      sale.status = "cancelled";
      return { status: 200, body: sale };
    }
    return { status: 404, body: { code: "not_found", title: "x", status: 404, detail: "no", type: "urn:x" } };
  });
  return { ...server, state, key };
}

/** The store's REST API: orders listed by `modified_after`, read and updated, behind basic auth. */
export async function wooStandIn({ consumerKey = "ck_test", consumerSecret = "cs_test" } = {}) {
  const state = { orders: [], updates: [], calls: [], down: false };
  const expected = `Basic ${Buffer.from(`${consumerKey}:${consumerSecret}`).toString("base64")}`;
  const server = await listen(({ method, url, headers, body }) => {
    state.calls.push(`${method} ${url.pathname}${url.search}`);
    if (headers.authorization !== expected) return { status: 401, body: { code: "woocommerce_rest_cannot_view" } };
    if (state.down) return { status: 500, body: {} };
    if (method === "GET" && url.pathname === "/wp-json/wc/v3/orders") {
      const since = url.searchParams.get("modified_after");
      const perPage = Number(url.searchParams.get("per_page"));
      const page = Number(url.searchParams.get("page"));
      const matching = state.orders.filter((o) => `${o.date_modified_gmt}` > since).sort((a, b) => a.date_modified_gmt.localeCompare(b.date_modified_gmt));
      return { status: 200, body: matching.slice((page - 1) * perPage, page * perPage) };
    }
    const one = /^\/wp-json\/wc\/v3\/orders\/(\d+)$/.exec(url.pathname);
    if (one) {
      const order = state.orders.find((o) => String(o.id) === one[1]);
      if (!order) return { status: 404, body: {} };
      if (method === "PUT") {
        order.status = body.status;
        state.updates.push({ id: order.id, status: body.status });
      }
      return { status: 200, body: order };
    }
    return { status: 404, body: {} };
  });
  return { ...server, state, consumerKey, consumerSecret };
}

let counter = 1000;
/** An order as the store sends it: strings for money, an integer quantity, two decimals. */
export function order(overrides = {}) {
  counter += 1;
  return {
    id: counter,
    number: String(counter),
    status: "processing",
    currency: "BRL",
    date_created: "2026-11-05T10:00:00",
    date_modified_gmt: "2026-11-05T13:00:00",
    line_items: [{ id: 1, name: "Café 500g", sku: "cafe-500", quantity: 2, subtotal: "20.20", total: "20.20", price: "10.10" }],
    ...overrides,
  };
}

export const product = (sku, name = sku) => ({ id: randomUUID(), kind: "product", sku: sku.toUpperCase(), name, unit: "UN", sale_price: "10.00", active: true });
