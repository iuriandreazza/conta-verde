const required = (env, name) => {
  const value = env[name];
  if (!value) throw new Error(`${name} is not set`);
  return value;
};

const apiAddress = (value) => {
  if (!/\/v1\/?$/.test(value)) throw new Error("CONTA_VERDE_URL is the API address the page showed, ending in /v1");
  return value;
};

/** The settings, all from the environment, and nothing logged. A secret is never printed, not even in an error. */
export function readConfig(env = process.env) {
  const storeId = required(env, "STORE_ID");
  if (!/^[A-Za-z0-9_-]{1,40}$/.test(storeId)) throw new Error("STORE_ID is 1 to 40 letters, digits, - or _");
  const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
  const customerId = required(env, "CONTA_VERDE_CUSTOMER_ID");
  const categoryId = required(env, "CONTA_VERDE_CATEGORY_ID");
  if (!uuid.test(customerId) || !uuid.test(categoryId)) throw new Error("CONTA_VERDE_CUSTOMER_ID and CONTA_VERDE_CATEGORY_ID are ids (uuid)");
  const paymentTermId = env.CONTA_VERDE_PAYMENT_TERM_ID || null;
  if (paymentTermId && !uuid.test(paymentTermId)) throw new Error("CONTA_VERDE_PAYMENT_TERM_ID is an id (uuid)");
  const secret = required(env, "WEBHOOK_SECRET");
  if (secret.length < 8) throw new Error("WEBHOOK_SECRET is the secret of the webhook, as the page showed it");
  return {
    storeId,
    customerId,
    categoryId,
    paymentTermId,
    webhookSecret: secret,
    contaVerde: { baseUrl: apiAddress(required(env, "CONTA_VERDE_URL")), apiKey: required(env, "CONTA_VERDE_API_KEY") },
    woo: { url: required(env, "WOO_URL"), consumerKey: required(env, "WOO_CONSUMER_KEY"), consumerSecret: required(env, "WOO_CONSUMER_SECRET") },
    port: Number(env.PORT || 8787),
    pollSeconds: Number(env.POLL_SECONDS || 60),
    stateFile: env.STATE_FILE || "./state.json",
    // Orders modified before this are not imported on the first pass.
    startFrom: env.START_FROM || new Date(Date.now() - 7 * 86_400_000).toISOString(),
  };
}
