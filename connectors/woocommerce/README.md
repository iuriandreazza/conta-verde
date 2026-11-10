# WooCommerce connector for conta-verde

A small program that connects one WooCommerce store to one company of [conta-verde](../../README.md). It is a **separate package**:
it imports nothing from the rest of the repository, has no dependencies (Node 22 and its standard library only), and speaks to the
product **only through the public `v1` API and the signed webhooks** ([ADR 0090](../../docs/adr/0090-ecommerce-connectors.md)).
The store's own keys live in this package's configuration and are never sent to conta-verde.

## What it does

- **Store → ERP.** Every minute it reads the orders the store modified since the last pass. An order that is **paid**
  (`processing` or `completed`) becomes an **open sale** (an order awaiting fulfilment) with `external_reference =
  woocommerce:{STORE_ID}:{order id}`, so reading it again never makes a second sale, and two stores of one company keep their
  numbers apart. An order **taken back** (`cancelled`, `refunded`, `failed`) cancels its sale while the sale is still open; if the
  sale is already completed the connector says so in its log and leaves it to you.
- **ERP → store.** When you **complete** the sale in conta-verde the order is marked `completed` in the store, and when you
  **cancel** it the order is marked `cancelled`. Sales that are not this store's (a point of sale, another store) are ignored, and an
  order already in the state asked for is left alone, which is what stops a cancellation made in the store from bouncing back.
- Prices are the **order's own**, never the product's today: each line's unit price is the store's subtotal over the quantity
  (rounded up to six decimals) with a discount in cents that makes the line worth exactly what the store charged.

## What it does not do (yet)

- **Shipping, fees and taxes are not imported**, only the items, so a sale's total is the items' total; the sale's note says the
  order's number. A currency other than BRL is refused.
- **The customer is one customer** you register in conta-verde for the store (`CONTA_VERDE_CUSTOMER_ID`), not one per buyer.
- A product is found by **SKU**: every item of an order needs a SKU that a product of conta-verde has (upper or lower case). An
  order that cannot be imported is logged and **read again for a week**, so registering the product is enough.
- It does not listen to the store's own webhooks: it polls (`modified_after`), which is simpler and safe to repeat.

## Setting it up

1. In conta-verde, **Configurações > Conectores**: create a connector for the store, give the public `https` address where this
   program will run (`https://…/webhooks/conta-verde`) and turn it on. The page shows once the **API key**, the **webhook secret**
   and the **API address**. Turning it off revokes the key and switches the webhook off.
2. Register in conta-verde the **customer** the store's sales are made to and pick the **revenue category**; copy their ids.
3. In the store, **WooCommerce > Settings > Advanced > REST API**: make a key with **Read/Write** permission.
4. Run it where the address of step 1 reaches it, with these environment variables:

   | Variable | What |
   |---|---|
   | `STORE_ID` | A short name for the store, 1 to 40 letters, digits, `-` or `_`. Part of every reference: **never change it** |
   | `CONTA_VERDE_URL` | The API address the page showed, ending in `/v1` (for example `https://erp.example.com/api/v1`) |
   | `CONTA_VERDE_API_KEY` | The key the page showed |
   | `WEBHOOK_SECRET` | The webhook secret the page showed |
   | `CONTA_VERDE_CUSTOMER_ID`, `CONTA_VERDE_CATEGORY_ID` | Ids of the customer and the revenue category |
   | `CONTA_VERDE_PAYMENT_TERM_ID` | Optional: a payment term id |
   | `WOO_URL`, `WOO_CONSUMER_KEY`, `WOO_CONSUMER_SECRET` | The store and its REST key |
   | `PORT` (8787), `POLL_SECONDS` (60), `STATE_FILE` (`./state.json`) | Optional |
   | `START_FROM` | Optional: orders modified before this ISO date are not imported on the first pass (default: seven days ago) |

   ```bash
   node src/main.js
   ```

   Put it behind a reverse proxy with TLS: conta-verde only sends webhooks to a public `https` address. `GET /healthz` answers `ok`.

## Tests

```bash
node --test "test/*.test.js"
```

The tests run against **local stand-ins** of the store's REST API and of conta-verde's `v1` (`test/standins.js`), written from each
one's public documentation, and check the signature against the scheme conta-verde documents.

## What was not validated

- **Never run against a real WooCommerce store.** The stand-in reflects the documented order fields (money as strings, an integer
  quantity, `dp=2`), the `modified_after` and `dates_are_gmt` parameters (supported by WooCommerce 5.8 and later, as far as could
  be found) and basic authentication over https; the store's behavior with plugins, other currencies, variations and a different
  SKU convention is untested.
- The repository's end-to-end script (`scripts/e2e-woocommerce.py`) runs this package against a **real conta-verde** with the
  store stand-in, which is the real proof of the half that is ours.
