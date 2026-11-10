/** An answer the API gave that is not a success: its problem document, never its credentials. */
export class ApiError extends Error {
  constructor(status, code, detail) {
    super(`conta-verde answered ${status} ${code}: ${detail}`);
    this.status = status;
    this.code = code;
  }
}

const MAX_TRIES = 4;
const MAX_WAIT_SECONDS = 60;

/**
 * The public `v1` API with a key, and nothing else: no private route, no cookie. A `429` waits as `Retry-After` says (at most a
 * minute) and tries again a few times.
 */
export class ContaVerde {
  constructor({ baseUrl, apiKey, fetchImpl = fetch, sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms)) }) {
    this.baseUrl = baseUrl.replace(/\/$/, "");
    this.apiKey = apiKey;
    this.fetch = fetchImpl;
    this.sleep = sleep;
  }

  async call(method, path, body) {
    for (let attempt = 1; ; attempt++) {
      const response = await this.fetch(`${this.baseUrl}${path}`, {
        method,
        headers: { authorization: `Bearer ${this.apiKey}`, ...(body === undefined ? {} : { "content-type": "application/json" }) },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: AbortSignal.timeout(15_000),
      });
      if (response.status === 429 && attempt < MAX_TRIES) {
        const wait = Math.min(Number(response.headers.get("retry-after")) || 1, MAX_WAIT_SECONDS);
        await this.sleep(wait * 1000);
        continue;
      }
      const text = await response.text();
      const json = text ? JSON.parse(text) : null;
      if (response.ok) return { status: response.status, body: json };
      throw new ApiError(response.status, json?.code ?? "internal", String(json?.detail ?? "").slice(0, 200));
    }
  }

  /** The product ids of these SKUs, matched exactly (the API's `q` finds substrings, and stores SKUs in upper case). */
  async productsBySku(skus) {
    const found = new Map();
    for (const sku of skus) {
      let after = "";
      do {
        const { body } = await this.call("GET", `/products?q=${encodeURIComponent(sku)}&active=true&kind=product&limit=200${after}`);
        for (const product of body.items) if (product.sku && product.sku.toUpperCase() === sku) found.set(sku, product.id);
        after = body.next_cursor && !found.has(sku) ? `&after=${encodeURIComponent(body.next_cursor)}` : "";
      } while (after !== "");
    }
    return found;
  }

  async saleByReference(reference) {
    const { body } = await this.call("GET", `/sales?external_reference=${encodeURIComponent(reference)}`);
    return body.items[0] ?? null;
  }

  makeSale(request) {
    return this.call("POST", "/sales", request);
  }

  cancelSale(id) {
    return this.call("POST", `/sales/${id}/cancel`, {});
  }
}
