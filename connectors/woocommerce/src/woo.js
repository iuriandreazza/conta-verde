/** The store's own REST API, with the store's own keys, which live only in this package's configuration. */
export class WooCommerce {
  constructor({ url, consumerKey, consumerSecret, fetchImpl = fetch }) {
    this.base = `${url.replace(/\/$/, "")}/wp-json/wc/v3`;
    this.authorization = `Basic ${Buffer.from(`${consumerKey}:${consumerSecret}`).toString("base64")}`;
    this.fetch = fetchImpl;
  }

  async call(method, path, body) {
    const response = await this.fetch(`${this.base}${path}`, {
      method,
      headers: { authorization: this.authorization, ...(body === undefined ? {} : { "content-type": "application/json" }) },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(8_000),
    });
    if (!response.ok) throw Object.assign(new Error(`the store answered ${response.status}`), { status: response.status });
    return response.json();
  }

  /** The orders modified since `since` (ISO 8601, GMT), oldest first, every page. Amounts come with two decimals (`dp=2`). */
  async ordersModifiedSince(since) {
    const orders = [];
    for (let page = 1; ; page++) {
      const batch = await this.call("GET", `/orders?modified_after=${encodeURIComponent(since)}&dates_are_gmt=true&orderby=date&order=asc&per_page=50&page=${page}&dp=2`);
      orders.push(...batch);
      if (batch.length < 50) return orders;
    }
  }

  order(id) {
    return this.call("GET", `/orders/${id}?dp=2`);
  }

  setStatus(id, status) {
    return this.call("PUT", `/orders/${id}`, { status });
  }
}
