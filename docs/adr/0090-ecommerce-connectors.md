---
title: "ADR 0090: E-commerce connectors: the settings page and the WooCommerce package"
status: accepted
date: 2026-11-10
tags: [adr, integrations, connectors, ecommerce, phase-8]
---

# ADR 0090: E-commerce connectors: the settings page and the WooCommerce package

## Context

[ADR 0086](0086-integrations-and-mobile-plan.md) planned a first e-commerce connector as a **separate package that uses only the
public API**, and a place in the settings where a company turns connectors on. [ADR 0087](0087-public-api-v1.md) built the `v1` API
and [ADR 0088](0088-webhooks.md) the webhooks it needs. This slice builds the catalogue and the settings page, and the first
package, for **WooCommerce**. It says what was built, the places where building it changed the plan (**Changed from ADR 0086**) and
what could not be validated without a real store.

## Decision

### Why WooCommerce first

It is open source and self-hosted like this product; its REST API (basic authentication over https with a key the store owner
makes) and its order objects need **no partner registration or OAuth application**, whereas the largest Brazilian hosted
platforms require one before any token exists. Several connectors may exist: another one is a new package, one entry in the
catalogue and, if it needs an event or a resource `v1` lacks, an **additive** change to `v1`.

### The catalogue and the connector (in the product)

- **A fixed catalogue in the domain** (`ConnectorKind`): for each kind its key, title, **the scopes of the key** (the least that
  covers what the package calls: WooCommerce needs `products:read` and `sales:write`), **the events of the subscription**
  (`sale.completed`, `sale.cancelled`) and where the package is in the repository. Adding a kind is a variant, a value of the table's
  `CHECK` and a package.
- **A connector** (`Connector`, `ConnectorService`, table `connectors`, routes `/companies/{id}/connectors`, the `members` area:
  owners and admins only, like the keys and the subscriptions it uses) is a kind, a name the person gives (one store; unique per
  company ignoring case; at most **10**), whether it is on, and the ids of the key and the subscription made for it. **It holds no
  credential of the store and never calls it.** Audited (kind, name, on or off, the two ids, never a secret). Composite foreign keys
  to `api_keys` and `webhook_subscriptions`; a `CHECK` says a connector that is on has a key and one that is off has none.
- **Turning it on** asks for the **public `https` address where the package listens** and makes only what any integrator could make
  by hand, **through the same services as their own pages** (`ConnectorKeys` and `ConnectorHooks`, narrow traits that
  `ApiKeyService` and `WebhookService` implement, so the connector service does not carry their type parameters): a key with the
  catalogue's scopes, named `Conector WooCommerce: <name>`, and a subscription to its events at that address. The answer carries the
  **two secrets, once**; the page shows them with the API address to paste. The subscription address follows the strict policy of
  ADR 0088 unchanged.
  - **Partial failure is designed for.** The installation's ability to seal the secret is checked **before** the key is made (without
    `FISCAL_SECRETS_KEY` nothing is made and nothing is left behind). The key is made, then the subscription; if the subscription
    fails, the key is revoked; the connector is then written **only if it is still as it was read** (`switch(connector,
    was_active)`), so of two people turning it on at once one wins and the other takes back what it made. What cannot be taken back
    is logged and never shown.
  - **Turning it off** revokes the key **first**, switches the subscription off (its address and events stay), and records it;
    asking again changes and records nothing. A failure after the key was revoked leaves a connector that still says it is on, which
    asking again finishes. **Turning it on again** makes a **new key**, switches the **same subscription** on at the address given
    and gives it a **new secret** (the old one signs for 24 hours, ADR 0088).
  - **The state shown is read every time**, never remembered: the person can revoke the key on **Chaves de API** or edit the
    subscription on **Webhooks**, so a connector that is on whose key or subscription no longer works is shown as **needs
    attention**, with the reason, and the way out is to turn it off and on.
- **Not built:** deleting a connector (turning it off is enough, and the trail is kept); renaming it.
- **The page** is **Configurações > Conectores** (`ConnectorsPage`): the catalogue's description of what a kind does and needs, a
  form to make a connector, a list with its state, **Ligar** (asks the address and shows the secrets once, with a link to the
  package's README), **Desligar** and **Histórico**. **Changed from ADR 0086:** ADR 0086 spoke of "Integrações" for the settings
  group; the group is **Configurações**, as for the keys and the webhooks, and the bank connections of ADR 0089 stayed under
  **Financeiro**, next to Conciliação.

### The WooCommerce package (`connectors/woocommerce`)

A **separate package**: its own README, tests and CI job, **no dependency** (Node 22 and its standard library) and so no lockfile; it
is not part of the pnpm workspace, **imports nothing from the repository** (a test reads every import) and speaks to the product
**only through `v1` with its key** (a test checks that every call it makes is a `v1` call) and receives only the signed webhooks.
The store's keys are in its own environment.

- **Store → ERP.** A pass every minute reads the orders modified since a cursor (`modified_after`, `dates_are_gmt`, with ten
  minutes of overlap, kept in a file). An order that is **paid** (`processing`, `completed`) becomes an **open sale**; one that is
  **taken back** (`cancelled`, `refunded`, `failed`) cancels its sale while it is open and, when the sale is already completed, says
  so in the log. It **polls** instead of listening to the store's webhooks: simpler, safe to repeat, and one fewer scheme to read
  without a store to try it on.
- **The reference is namespaced**: `woocommerce:{STORE_ID}:{order id}` (at most 100 characters), because two stores of one company
  both have an order 123 and the claim of ADR 0087 is per company. `STORE_ID` must never change.
- **The request is made from the order alone**, deterministically, so reading an order again is the same request (a `200` with
  the same sale) and an order the store **edited after it was imported** is told apart by the API (`409`): the package logs it and
  goes on, never loops. Prices are the **order's own**, never the product's today.
- **Money** is whole cents in `BigInt`, from the store's **string** fields (`subtotal`, `total`; the line's `price` is not used),
  with the rule of ADR 0057: the unit price is the subtotal over the quantity **rounded up to six decimals** and the cents over
  are taken as a **discount**, so the ERP's `round_to_cents(quantity x price) - discount` is exactly what the store charged (a
  property test with the ERP's rule written independently).
- **Not imported, written in the README:** shipping, fees and taxes (only the items are), a currency other than BRL, one customer
  per buyer (the sales go to **one customer the person registers for the store**, and one revenue category, both ids in the
  package's settings), variations beyond what the SKU says. A product is found by **exact SKU** (the API's `q` finds substrings, so
  the package reads the page and compares, in upper case as the ERP keeps it). An order that cannot be imported (a SKU not registered
  yet) is logged once and **read again for a week**, so registering the product is enough.
- **ERP → store.** The subscription brings `sale.completed` and `sale.cancelled`. A delivery is checked **before it is read**: the
  signature exactly as ADR 0088 says (the key is the secret's own text, several signatures separated by a space, constant time, on
  the raw body), a timestamp within five minutes, a `Webhook-Id` not seen (the last 1,000 kept). A sale whose reference is not this
  store's (a point of sale, another store) is **accepted and ignored**. A completed sale completes its order; a cancelled one cancels
  it (unless the order is already `completed` in the store: a shipped order is not taken back from here); **an order already in the state asked for is left alone**, which is what ends the loop when the cancellation began in the
  store. When the store cannot be reached the answer is `502` and the delivery is **not** remembered, so conta-verde sends it again
  (ADR 0088's nine hours of retries).
- A `429` is waited out as `Retry-After` says (at most a minute, four tries); a failure of the API itself fails the pass, which is
  tried again from the same cursor.

## What the tests prove

- `contract_integrations::runs_connectors` on **both adapters** (memory and Postgres) over the **real** key and subscription
  services: the name, the limit of 10, the key and the subscription made with the catalogue's scopes and events, a failed
  subscription taking its key back, turning on twice refused and making nothing, another company's connector 404, the loser of two
  at once, off revoking the key and keeping the subscription, on again with a new key and the same subscription, the trail without
  a secret.
- API tests: the whole cycle through the routes, **the key opens `/v1/products` and not `/v1/parties`** and stops working when the
  connector is turned off, a key revoked elsewhere is shown as needing attention, only owners and admins reach the routes.
- The package's 28 tests against local stand-ins of the store and of `v1`, including the signature (wrong secret, changed body,
  replaced secret, five minutes, replay), the money property, the loop guard, a week of retries and the isolation checks.
- **`scripts/e2e-woocommerce.py`** runs the package against the **real API, Postgres and worker** with the store stand-in: a paid
  order becomes one open sale with its reference and the order's total, completing it makes the real worker send a signed webhook
  that the package verifies and turns into a completed order in the store, a cancelled order cancels its open sale, and turning the
  connector off stops the key. Repeatable by hand; it is not a CI job (it needs the API running).

## Not validated

- **The package has never run against a real WooCommerce store.** The stand-in reflects the documented fields (money as strings, an
  integer quantity, `dp=2`), basic authentication, and `modified_after` with `dates_are_gmt` (supported since WooCommerce 5.8, as
  far as could be found; a report says `dates_are_gmt` can be ignored together with `modified_after`, which the overlap and the
  idempotent reference make harmless). Plugins, variations, other currencies, taxes shown with prices and the store's SKU convention
  are untested.
- The e2e uses the store stand-in; what is proved with the real product is the half that is ours.

## Consequences

- A company can have several connectors, of one kind or of several, each with its own key (so each can be revoked alone).
- Everything a connector can do is in `v1` and the webhooks: nothing about it is private, and the page shows the same key and
  subscription as the other two pages.
- A key a connector needs more of is a change of the catalogue (and of the scope of the key of existing connectors by turning them
  off and on).

## Revisit when

- A real store is available: run the package, fix what the stand-in got wrong and say so here.
- A second connector exists: see whether the catalogue needs descriptions in data instead of code.
- Shipping or taxes must reach the books: that needs a decision on where they go (a product, a category), not a default.
