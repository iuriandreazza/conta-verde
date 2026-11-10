---
title: "ADR 0087: The public API v1: keys, scopes, limits, the document and the first resources"
status: accepted
date: 2026-11-10
tags: [adr, api, integrations, security, phase-8]
---

# ADR 0087: The public API v1: keys, scopes, limits, the document and the first resources

## Context

[ADR 0086](0086-integrations-and-mobile-plan.md) planned Phase 8 and settled what "`v1` frozen" means, how keys and limits work
and why the document is written by hand. This is its second slice: the code. It records what was built, and the few places where
building it changed the plan (each is marked **Changed from ADR 0086**).

## Decision

### The keys

- A **key** (`ApiKey` in `domain`, `ApiKeyService` in `application`, table `api_keys`) belongs to one company, has a name, 1 to
  10 **scopes**, an expiry that is an **instant** (Unix milliseconds) or none, a `revoked` flag, who made it and when, the key it
  replaced (`rotated_from`) and the **day** of its last use. At most **50 live keys per company**: a revoked key stays listed so that its
  history can be reached, but takes no place.
- The **secret** is `cvk_` and a fresh token of `SecretService::new_token` (256 bits, 64 hexadecimal characters). It is shown
  **once**, in the answer that makes the key (`IssuedKey` hides it from `Debug`); the database keeps its **SHA-256 digest**
  (unique, `CHECK` of 64 hex characters) and a **label**, `cvk_` and the first six characters, to tell keys apart. No port gives a
  digest back, and the audit trail never holds the secret or its digest (a test searches it).
- **Scopes** are `{area}:{read|write}` over `parties`, `products`, `finance`, `sales` and `purchases` (`API_KEY_AREAS`);
  `write` includes `read` and is stored without the `read` of its area. `members`, `accounting` and `fiscal` are **refused by
  the domain and by a `CHECK`** (`scopes <@ ARRAY[...]`), so not even a direct insert makes such a key.
- **Rotation** makes a new key with the same name and scopes and **the same life** as the old one had (a key made to expire never gives a
  permanent one), and gives the old one an expiry of the earlier of its
  own and now plus 0 to 168 hours (24 by default), **in one transaction with both audit entries** (`ApiKeyRepository::rotate`).
  A revoked or expired key is not rotated (`409`); a write of a key's life only ever goes towards less (`revoked OR`, `LEAST` of the expiries), so a
  rotation that read the key before another admin revoked it cannot bring it back. **Revocation** is at once and idempotent (asking again writes nothing). A key
  is never edited and never deleted.
- **Who may manage keys**: `/companies/{id}/api-keys` (list, create, `rotate`, `revoke`, `history`) is in the SPA's API, in the
  `members` area, so only owners and admins reach it, and is covered by the existing walk of every company route.
- **Authentication** (`ApiKeyService::authenticate`): a secret without the prefix, an unknown one, a revoked one and an expired
  one are **the same refusal** (`AuthError::Unauthenticated`), so a secret cannot be probed. The day of use is written only when it
  changes (a failed note never fails the request).

### The actor

`Actor` has a second variant, `ApiKey { id, name }`, with `id()`, `name()`, `user_id()` and `kind()`; `audit_log.actor_kind` is
widened to `('user', 'api_key')` by a new migration, `postgres_audit` writes and reads the kind, and the history answer says
`"api_key"`. The places that took a person out of an `Actor` are three kinds: the records that point at a **person** (an
invitation, a mailed document, a fiscal event) now **refuse a key** (`AuthError::NotAllowed`, which no route reaches since a key
has no `members`, `fiscal` or document-mail route); the columns that keep "the person in whose name later work is done"
(a charge's issuer, a contract's saver, a recurrence's, a quote's sender, a fiscal document's submitter) store the id and name of
whoever acted and read back as a person, which no `v1` write reaches; and the audit trail, which keeps the kind.

### The `v1` surface

- `GET /v1/openapi.json` (no key) and, with a key: `parties` (list, get, register), `products` (list, get) and
  `products/{id}/stock`, `categories` and `payment-terms` (lists, what a sale has to name), `sales` (list, make, get,
  `complete`, `cancel`) and `sales/{id}/charges` (the charges that collect a sale and whether they were paid). Routes are
  `/v1/...` **without the company**: the key is the company.
- **Own types** in `crates/api/src/v1/resources.rs`, built from the domain types and independent of the SPA's DTOs. The handlers
  call the **same services** as the SPA (`state.sales.register`, `complete`, `cancel`, `state.parties.register`, ...), so every rule
  and audit entry applies and the key is the actor.
- **Areas of a route** are one table, `v1::ROUTE_AREAS`, by exact path; a route absent from it is `403` to every key. A stock
  position needs `purchases:read` and a sale's charges `finance:read`, because stock and charges live in those areas.
- **The gate** (`v1::gate`, a `route_layer`) runs before any handler: it reads **only** `Authorization: Bearer` (a cookie is never
  looked at, and a bearer token opens none of the SPA's routes), authenticates, **takes a token from the key's bucket**, checks the
  scope, and puts `Principal { company, actor }` in the request. Every answer carries `RateLimit-Limit`, `-Remaining` and `-Reset`;
  an error of any kind leaves as **`application/problem+json`**: the gate rewrites the `{"error": ...}` of the use cases and of the
  extractors into `type` (`urn:conta-verde:problem:<code>`), `title`, `status`, `detail` and `code`
  (`invalid_request`, `unauthenticated`, `insufficient_scope`, `not_found`, `conflict`, `rate_limited`, `internal`), so there is one
  shape whatever stopped the request. List parameters that are not known are `400` (`deny_unknown_fields`).
- **Not found** answers for another company's record exactly like an unknown one (`404`), and a reference lookup never crosses
  companies.

### Rate limits

A **token bucket per key in the memory of the process** (`v1::rate_limit`): capacity and refill of `API_RATE_LIMIT_PER_MINUTE`
(300 by default, `0` refused at start). With several instances the real limit of a key is that many times the one set, and a
restart forgets the buckets (the buckets that are full again are dropped when there are more than 1,024). The time is
`V1Clock` (Unix milliseconds, replaced by tests), which also judges a key's expiry: **a new place that reads a clock** (the others
being the passes of ADRs 0033/0040/0048, the job queue and the document e-mail limits).

### Idempotency: a claim before the sale

**Changed from ADR 0086.** The plan put `external_reference` on the sale with a unique index. A claim made **before** the sale, in
its own table, does the same job without touching `Sale`, its twelve constructors and its two adapters, and it closes the race
the same way:

- `sale_claims (company_id, reference, sale_id, request)` with the primary key `(company_id, reference)`, a unique `(company_id,
  sale_id)`, and no foreign key (the sale is written after its claim). `SaleClaimRepository::claim` inserts `ON CONFLICT DO NOTHING`
  and **gives back the claim that holds**: a new one for the first caller, the first one for every later caller.
- `SaleService::register_external` **checks the order first** when the reference is new (a request a rule refuses never takes the
  reference, so a client that fixes its request may send it again under it), then claims with a **fresh sale id**, then: another `request` text under the same reference is
  `IdempotencyMismatch` (`409`); a sale with the claim's id means "done before" (`200`); otherwise it **writes the sale with the
  claim's id**. So a crash between the claim and the sale is finished by the retry (the sale the claim promised), two requests at
  once end in one sale (the loser finds the winner's sale, or its duplicate id write fails and it reads it), and only a storage error after the claim can leave a claim with no sale, which the next try makes.
- `request` is the canonical JSON of the request (the typed body re-serialized, so whitespace and key order do not matter): the
  comparison is exact, with no hash to collide. A decimal written another way (`10.5` and `10.50`) is another request.
- `GET /v1/sales?external_reference=` finds the sale, and every sale of a company that has a reference says it
  (`SaleClaimRepository::reference_of`). A party is unique by its document and a product by its SKU, so a repeat is a plain `409`.

### The document and its tests

`docs/api/v1/openapi.json` (OpenAPI 3.1, written by hand), served by `GET /v1/openapi.json`, and `docs/api/v1/openapi.frozen.json`,
its copy at the first release. Tests (in `crates/api/src/v1/tests.rs`, no new crate): the routes read from the source of `routes`
are exactly the document's paths and methods, and every route but the document's has an area a key may have; **every response of the
end-to-end tests conforms to its schema** (a small validator in the test code: `$ref`, `type`, `properties` that are all there are,
`required`, `items`, `enum`, `oneOf`, `uuid` and `date`), so a field the handler sends and the document does not say, or the other way
round, fails; and the document **contains** the frozen copy (every path, method, parameter, property and enum value; a request may not
require more). **Changed from ADR 0086:** `GET /v1/sales?updated_since=` and the sale's `updated_at` are **not in this slice**: they
are the tool for reconciling the webhooks, so they land with them (slice 3).

### The page

**Configurações > Chaves de API** (`ApiKeysPage`, `/companies/{id}/api-keys`; the nav item is in the settings area): the list with what
each key can do, its state, its last use and its trail; **Nova chave** opens the global sheet (`ApiKeyCreator`: a name, what it reaches in
each of the five areas, how long it lives) and ends on the secret, shown once, with a copy button and the address of the API and of the
document; **Girar** asks how long the old key goes on, **Revogar** asks to confirm. Only a person who writes the `members` area sees the
buttons.

## Consequences

- `sales:write` reaches more than the sale: completing and cancelling one writes its receivable and its stock exits, through the same use cases as the
  web app. That is what completing a sale is; a key that must not bill is given `sales:read`.
- A key reaches only what its scopes name, only its company, never `members`, `accounting` or `fiscal`, and leaves its name in the
  trail of what it wrote; a leaked key is revoked at once and a leaked database holds no usable secret.
- The SPA's DTOs and the `v1` types are two sets for the same resources; the tests hold the second to its document, not to the first.
- The rate limit is approximate across instances. Wrong keys are not throttled (no client address behind a proxy); each costs one
  indexed query.
- `request` in a claim is kept as typed text (up to 500,000 characters): an order's request, not a secret. Claims are never deleted.
- **Not validated against anything real**: nothing here talks to an outside service; the `v1` contract is exercised through the
  router with the in-memory ports, and the Postgres adapters of keys and claims by the shared contracts (`contract_integrations`).

## Revisit when

- A resource `v1` lacks is needed (returns, quotes, stock movements): add it, additively, to the table, the document and the tests.
- The loss of an exact distributed limit shows: put the counter in the proxy, or in Postgres.
- A key must outlive nothing it should not (the person who made it leaves the company): today it is the company's and is revoked by
  hand.
