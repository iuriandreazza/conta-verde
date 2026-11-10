---
title: "ADR 0086: Integrations and mobile: the plan for Phase 8"
status: accepted
date: 2026-11-10
tags: [adr, api, integrations, webhooks, pwa, phase-8]
---

# ADR 0086: Integrations and mobile: the plan for Phase 8

## Context

Phase 8 has five roadmap items: a frozen public API with keys and rate limits, webhooks, an automatic bank statement
sync, a first e-commerce connector and a PWA. Two accepted ADRs describe them from before the code existed:
[ADR 0011](0011-bank-data-ingestion.md) (accepted) and [ADR 0015](0015-public-api-and-webhooks.md) (proposed). Several of
their promises no longer match the repository, and a phase starts by accepting the ADRs it depends on, so this ADR
settles each point against what the code is today and **amends** both (they stay; the deltas are here).

What the code is today, and ADR 0015 did not know:

- **There is no `/api/v1`.** The routes are `/companies/{company_id}/...`; the SPA reaches them through `/api` with the
  prefix stripped by its proxy. About 170 routes, changing every phase, authenticated by a **session cookie** only.
- **No generated client and no OpenAPI.** `web/src/api.ts` is kept in step with the Rust DTOs by hand
  ([`CLAUDE.md`](../../CLAUDE.md)). Errors are `{"error": "message"}`, not RFC 9457. No creates take an idempotency key.
- **Who acts is a user.** `Actor` has one variant, `audit_log.actor_kind` is `CHECK (actor_kind IN ('user'))`, and
  `postgres_audit::insert`, the history route and about a dozen other places destructure `Actor::User`.
- **What a role may do is the route's:** `CompanyAccess` reads the area from the segment after `{company_id}`
  (`SEGMENT_AREAS`, [ADR 0026](0026-membership-roles-and-permissions.md)) and the method gives read or write.
- **Background work exists** ([ADR 0059](0059-job-queue.md)): a table of jobs with fixed-arity plain-text arguments,
  at-least-once delivery, doubling retry capped at one hour, five tries, finished jobs deleted. There is no screen for
  failed jobs.
- **Audit is atomic with the write; nothing else is.** Every write carries an `AuditEntry` stored in the same
  transaction. Each in-memory repository keeps its own audit log inside its own state.
- **Secrets are never stored raw:** a token is shown once and only its SHA-256 digest is kept
  ([ADR 0027](0027-invitations.md), [ADR 0038](0038-quotes.md)). Credentials of external services come from the
  environment ([ADR 0044](0044-collection-module-plan.md), [ADR 0075](0075-fiscal-gateway-port.md)).
- **The server reads a clock in very few places** (recurrences, contracts, reminders, the job queue, document e-mail
  limits).

## Options considered

### Which API is "public"

| Option | Pros | Cons |
|--------|------|------|
| Freeze the SPA's API (what ADR 0015 and principle 7 of the macro plan say) | One contract; the SPA proves it | 170 routes, built in the open and still moving, would stop moving; the DTOs carry what the screens need, not what an integrator should depend on; every Phase 9 refactor would need a version |
| A separate frozen `/v1` of curated resources, with its own DTOs, calling the same use cases (chosen) | Small surface that can honestly be kept for a year; the SPA stays free; no rule is skipped because the use cases are the same | A second set of DTOs for those resources; "no private API" becomes "the SPA's API is not a contract" |
| GraphQL | Flexible queries | Harder caching, authentication and limits; less familiar to integrators |

### Where the webhook's event comes from

| Option | Pros | Cons |
|--------|------|------|
| An outbox row written in the same transaction as the business write | No event lost on a crash | Every write method of both adapters would carry an event as well as an audit entry (the signatures of about thirty port methods change), and the in-memory adapters keep one audit log **per repository**, so there is nowhere common to write it |
| The use case publishes after its write succeeded (chosen) | One place per event; both adapters behave alike; no port changes | A crash between the commit and the publish loses that event (see Consequences) |
| Derive events by scanning `audit_log` | Atomic with the write | Postgres only; `sequence` is an identity, so a transaction that commits late can be skipped by a cursor; the memory adapters cannot do it |

### What a webhook's signing secret is

| Option | Pros | Cons |
|--------|------|------|
| Only its digest, like a token | Nothing recoverable | The server must sign, so it must know the secret: impossible |
| Derived from a master key and the subscription's id and version | Nothing secret stored; no new crate | The person **cannot choose or edit it** (a requirement: the secret is editable in the interface, so a receiver that already has one can keep it), and a leaked master key exposes every secret ever issued |
| **Sealed at rest with the `SecretCipher` that already exists** ([ADR 0085](0085-gateway-settings-per-company.md), AES-256-GCM through `ring`) (chosen) | Any value, generated or typed by the person, **editable in the interface**; no plain secret in the database; **no new crate and no second mechanism**: the company and the field are bound as associated data | A key to guard and back up (`FISCAL_SECRETS_KEY` today); losing it means every secret is typed again; rotating the key is not built there either |

### Idempotency of a create

| Option | Pros | Cons |
|--------|------|------|
| An `Idempotency-Key` header with a table of stored responses (what ADR 0015 promised) | Works for every POST | A crash after the write and before the response is stored leaves a claim that cannot say whether the write happened; another table, kept and pruned |
| A natural key on the record, unique per company (chosen) | Crash-proof (the unique index is the claim); the record can be found again by it; it is the repository's own idiom (a purchase invoice's access key, an import's `reference`, a contract's cycle day) | Only for the records that get one |

## Decision

### 1. The slices, in order

Each slice is one pull request with its own ADR (numbers taken when written) and updates the roadmap, the ADR index and
the rules in `CLAUDE.md`.

1. **This ADR**, and the amendments to 0011 and 0015. Documents only.
2. **Public API `v1`**: keys, scopes, rate limits, the OpenAPI document and its drift tests, the first resources, the
   page for keys.
3. **Webhooks**: subscriptions, signed deliveries, retries, the delivery log, the page.
4. **Automatic bank statement sync** through an aggregator port.
5. **E-commerce connectors**: the settings page where a company **activates** one, and the first connector
   (WooCommerce), a separate package.
6. **PWA**.

The order follows the dependency: the connector needs the API and the webhooks; the sync and the PWA need neither and
could move earlier. They stay where the roadmap put them unless a slice says why not.

### 2. What "`v1` frozen" means

- **A prefix and a curated set.** The routes are `/v1/{resource}`, **without the company in the path**: a key belongs to
  one company, which is where the request is made. The SPA's cookie routes are **not** under it and are **not a contract**
  (principle 7 of the macro plan is amended: the SPA and `v1` are two front doors to the same use cases, so no rule, check
  or audit entry can be skipped through either). The exit criterion "without any private endpoint" is this distinction.
- **Its own types.** `v1` has request and response DTOs of its own (`crates/api/src/v1/`), written from the domain types,
  never the SPA's DTOs. A change to a screen cannot change the contract.
- **Disjoint authentication.** A `v1` route reads only `Authorization: Bearer <key>` and ignores cookies; the SPA's routes
  read only the cookie and ignore the bearer. A request with both is answered as what its path says. No CSRF surface
  exists on `v1`.
- **The rule of compatibility is additive only.** Within `v1` the API may gain endpoints, optional request fields,
  response fields and query parameters; it may not remove or rename any of them, change a type, make an optional request
  field required, tighten what a field accepts or change what an existing value means. Clients must ignore response
  fields they do not know **and treat an enum value they do not know as "other"**, so a new value is not a break. A
  breaking change is a `v2`, and `v1` is kept for **at least one year** after `v2` ships, announced by the `Deprecation`
  and `Sunset` headers.
- **The document is served and checked.** `GET /v1/openapi.json` (no key) serves the OpenAPI 3.1 document checked in at
  `docs/api/v1/openapi.json`. It is written by hand, **not generated** (no `utoipa`: it would need an annotation on every
  handler and a derive on every DTO for fifteen endpoints, and still would not catch a field the handler leaves out).
  Three tests keep it honest, with no new crate:
  1. every `/v1` route of `router` (read from its source, as the accountant test reads every route) is in the document
     with the same method, and every operation of the document is a route;
  2. for every operation the tests exercise, the **shape of the real JSON** is checked against the document's schema
     (no key outside `properties`, every `required` key present, enum values and formats respected); the small
     validator for this is written in the test code (`$ref`, nullable `type` arrays, `oneOf`, formats), since
     `serde_json` is there already and no schema crate is added: it is real work and the slice budgets for it;
  3. the document **contains** `docs/api/v1/openapi.frozen.json`, a copy made when `v1` first shipped (every path and
     method, every property, every enum value; a request's required list may not grow). The frozen copy is a tripwire,
     not a lock: changing it is a visible act that a review must question, and a PR that does so says it breaks `v1`.
- **Errors are RFC 9457** (`application/problem+json`) on `v1` only: `type` (`urn:conta-verde:problem:<code>`), `title`,
  `status`, `detail` and a stable machine `code` (`invalid_request`, `unauthenticated`, `insufficient_scope`,
  `not_found`, `conflict`, `rate_limited`, `internal`; finer codes may be added). The SPA's `{"error": ...}` is unchanged.
  A storage error is a `500` with a generic `detail`, as everywhere ([`CLAUDE.md`](../../CLAUDE.md)).
- **Conventions carried from the product:** JSON; decimals as strings, never numbers ([ADR 0003](0003-money-as-decimal.md));
  dates `YYYY-MM-DD`; moments are **RFC 3339 UTC strings**; lists take `limit`, `after`, `q`,
  closed-value filters and `sort`, with the keyset cursor ([ADR 0018](0018-list-endpoints-search-filter-sort.md)); an
  unknown parameter is `400`, never ignored.
- **Idempotency is a natural key** (amending 0015's `Idempotency-Key`): a party is found again by its document, a product
  by its SKU, and a **sale gets an optional `external_reference`** (1 to 100 characters, no control characters, unique
  per company, a partial unique index). The sale keeps a digest of the normalised request that made it.
  Creating a sale whose reference exists **with the same request** returns **that sale** (`200` instead of `201`); with a
  **different** request it is `409 idempotency_mismatch`, so a connector can see that an order changed. Two creates at once
  race on the unique index and the loser reads the winner. `GET /v1/sales?external_reference=` finds it. This is what lets
  a connector replay an order without making it twice, even across a crash. A party or product that already exists is a
  plain `409 conflict` (its document or SKU is unique); the client finds it with `?document=` or `?sku=`.
- **Sales gain `updated_at`** (slice 2, bumped by every write including `set_status` and a return) and
  `GET /v1/sales?updated_since=`, a keyset-ordered list, which is what "reconcile with the reads" in the consequences
  means: today `sales` only has `created_at`.
- **The first resources**: parties, products with their stock position, sales (create, read, complete, cancel) and the
  status of a sale's charges, with reads on all and writes through the same use cases as the web (every rule applies and
  every write is audited with the key as the actor). Webhook payloads reuse the `v1` DTO of the resource, so a consumer
  parses one shape, and the document describes them (`webhooks` of OpenAPI 3.1).

### 3. API keys

- A key belongs to **one company**, has a name, an optional expiry day and a set of **scopes**. Its secret is made by
  `SecretService::new_token` with the prefix `cvk_`, **shown once** when created, and only its **SHA-256 digest** is
  kept (`digest_token`, unique); the first characters are kept as a **label** so a person can tell keys apart. A lookup
  is one indexed query by digest.
- **Scopes map onto the existing table**: a scope is `{area}:{read|write}` (`sales:write`, `products:read`), `write`
  includes `read`, and a route's area and method decide what it needs, exactly as `CompanyAccess` does for a role. `v1`
  has its **own table of resource to area** (a product's stock position needs `purchases:read`, a sale's charges
  `finance:read`, because stock and charges live in those areas today). The
  areas a key may be given are `parties`, `products`, `finance`, `sales` and `purchases`; **never `members`,
  `accounting` or `fiscal`** (they hold who belongs to the company, the books' closing and documents with legal weight).
  A key refused for a scope is `403 insufficient_scope`: the holder owns the key, so there is nothing to hide.
- **Keys are managed by owners and admins only**: the routes (`/companies/{company_id}/api-keys`, in the SPA's API, never
  in `v1`) sit in the `members` area, and so do the webhook subscriptions. A key cannot make or change keys. **A key belongs to the company, not to the admin who made it**: it survives that person's removal or demotion (revoke it by hand; the page lists who made each), and a company has at most 50.
- **Rotation** makes a new key with the same name and scopes and sets the old one's expiry (an **instant**) to the earlier
  of its own and now plus 0 to 168 hours (24 by default), so a client can switch without a gap; a revoked or expired key
  cannot be rotated. **Revocation** marks the key (`revoked`) and is immediate. Beyond its expiry, revocation and name a
  key is never edited and never deleted; creating, rotating and revoking are audited
  (`AuditEntity::ApiKey`), and the audit entry never holds the secret or its digest.
- **The actor is a variant**: `Actor::ApiKey { id, name }`, with `Actor::id()` and `Actor::name()` so no caller has to
  match. `audit_log.actor_kind` widens to `('user', 'api_key')` in a new migration; both adapters write and read it;
  the history shows who it was. The columns that keep "the person in whose name later work is done" (a charge's issuer,
  a contract's last saver, a quote's sender, a fiscal document's submitter, a document e-mail's sender) **stay for people
  only**: no `v1` write of this phase reaches them, and a slice that wants one to must give those columns a kind first. The
  places to change are `postgres_audit::insert` (it writes `'user'` literally), `history.rs` (it answers `"user"`
  literally), the in-memory log and the web history component.
- **Rate limits** are a **token bucket per key, in the memory of the process**: a capacity and a refill of
  `API_RATE_LIMIT_PER_MINUTE` (300 by default, `0` is refused) a minute. **Said plainly: with N instances behind a
  balancer a key's real limit is N times that**, and a restart forgets the buckets. A Postgres-backed counter would turn
  every request into a write; if an operator needs an exact limit, the proxy in front of the API is the place for it.
  Every `v1` answer carries `RateLimit-Limit`, `RateLimit-Remaining` and `RateLimit-Reset`; a refused request is `429
  rate_limited` with `Retry-After`. The map of buckets drops a key's entry once it has been full for a minute. Wrong keys are not throttled (there is no
  client address behind a proxy, the same gap as the login's, [`CLAUDE.md`](../../CLAUDE.md)): a 256-bit secret cannot be
  guessed, and a wrong one costs one indexed query, which is accepted.
- **Two new places read a clock**, both passed in as `now` by the caller so a test moves it
  ([ADR 0065](0065-document-mail-limits-and-log.md)): a key's expiry (judged at each request) and its **last use**,
  written as a **day** only when the day changes (bookkeeping, not audited, so reading never writes more than once a
  day). The rate limiter uses a monotonic `Instant` and no wall clock.
- `access::area_of` and the walk over every route of the router keep covering the SPA's routes **and skip `/v1`**
  (`area_of` has none for it, so the walk would expect `403`); `v1` has its **own** table
  of segments and areas and its **own** walk, which reads the `v1/` sources as well as `lib.rs`: a test goes through every `/v1` route and expects `401` with no key,
  `403` with a key of a lacking scope, `403` for a write with a read-only key, `404` for another company's record, and
  `429` past the limit.

### 4. Webhooks

- A **subscription** is a company's, with an `https` URL, a list of event types (or all) and an `active` flag, at most 20
  per company, audited (without the secret). Its **signing secret is editable in the interface**: the
  server makes one at random when the subscription is created (`whsec_` and 32 random bytes in base64) and **shows it once**;
  the owner or admin can **replace it at any time** with a value of their own (the rule of [ADR 0085](0085-gateway-settings-per-company.md):
  printable ASCII, no spaces, 8 to 200 bytes; the page also offers "generate a new one"), for a receiver that already has
  one. Replacing it keeps the previous one signing too for 24 hours, so a receiver can switch without a gap. It follows the
  repository's convention for secrets, **write-only**: it is **sealed with the existing `SecretCipher`** (company and field
  as associated data) and the API never returns it, only whether it is set; a person who lost it replaces it. A change is
  audited **without the value**. The **key is the installation's** (`FISCAL_SECRETS_KEY` of ADR 0085, 64 hex characters;
  slice 3 may add `SECRETS_KEY` as the general name with the old one as a fallback). **Without it webhooks are off** and the
  page says so (the pattern of `PAYMENT_PROVIDER`); losing it means every secret has to be replaced, never that data is lost.
  A secret that does not open is a `409` naming the subscription, as ADR 0085 does. The aggregator of slice 4 and any later
  adapter that needs a per-company secret use the same port.
- **The catalogue of events** starts with `sale.created`, `sale.completed`, `sale.cancelled`, `charge.paid`,
  `stock.below_minimum` and `webhook.test`. It only grows. `stock.below_minimum` is derived on read in the product (it is
  never stored), so the event fires on a **crossing**: when a movement takes a product that has a minimum from a balance
  at or above it to one below it, decided by the stock use case from the balance before and after.
- **A delivery** is a `POST` of `{ "id", "type", "created_at", "company_id", "data" }` with the headers
  `Webhook-Id` (the event's, the same on every try), `Webhook-Timestamp` (Unix seconds, **new on every try**) and
  `Webhook-Signature: v1,<base64 HMAC-SHA256 of "{id}.{timestamp}.{body}">` (several space-separated `v1,` values while a
  previous secret is still valid), the encoding of the Standard Webhooks specification so its libraries verify it. A receiver **verifies the signature in
  constant time, refuses a timestamp more than five minutes off and remembers ids it has seen**: that is the replay
  protection. Success is any `2xx` within 10 seconds; the response body is never read or kept.
- **Retries go through the job queue** ([ADR 0059](0059-job-queue.md)): a `deliver_webhook` job carries only the
  **delivery id** (its arguments are fixed-arity plain text and a finished job is deleted), and the payload, the status
  and what happened live in a **delivery log** table (`pending`, `delivered`, `failed`, the tries, the last status code
  and a short reason, never a response body), pruned after 30 days. The queue's rule of five tries with the wait capped at
  an hour is **not enough** for a subscriber that is down overnight, so the maximum of tries becomes a property of the
  **job kind** (`JobKind::max_attempts`: 5 for e-mail as before, **15 for webhooks, about nine hours in all**, the waits
  being 1, 2, 4, 8, 16, 32 and then 60 minutes; the runner's crash guard uses the kind's limit too, and the table's `CHECK`
  on `kind` gets the new value in a migration). **A slow subscriber must not hold up the queue**: the worker is one
  sequential loop, so webhook deliveries run in a bounded set of concurrent tasks (the first slice that needs it
  changes `JobRunner`), with the 10 second timeout. A failed
  delivery can be sent again by hand, and "send a test event" makes a `webhook.test` delivery that shows in the log.
- **The event is published by the use case after its write succeeds** (a new `EventPublisher` port, with both adapters:
  it stores the event, one delivery per matching subscription, and queues their jobs). If publishing fails after the
  commit it is **logged and swallowed**: the write is not undone and the caller is not told it failed. An event has a
  **deterministic id** (a UUID v5 of the company, the type, the record and the transition, with a unique index), so a
  retried step ("asking again finishes it": completing a sale, a repeated payment notice) publishes the same event and
  stores it once; slice 3's ADR lists **every path** that makes each event (creating a sale, converting a quote, a
  contract's cycle, the point of sale, a webhook's payment) so none is missed. See the consequences for what that costs.
- **SSRF.** The URL is checked when the subscription is made (https only, no credentials in it, a host, no IP literal in a
  private range) **and again at every delivery**, because a name can change what it points at: the delivery resolves the
  host with a timeout and **refuses the name if any address it resolves to is blocked**. IPv4-mapped IPv6 addresses are
  turned into their IPv4 before they are classified, and the blocked ranges are a table with a test per row: loopback,
  `0.0.0.0/8`, private, link-local (the metadata address), carrier-grade NAT `100.64/10`, `192.0.0.0/24`, `198.18/15`,
  documentation, `240/4`, multicast, broadcast; and for IPv6 loopback, unspecified, unique-local, link-local, multicast,
  documentation, NAT64 `64:ff9b::/96` and 6to4 `2002::/16`. It connects **to the address it checked** with a client made
  for that delivery (the name pinned to it) and **no proxy** (`no_proxy()`: an environment proxy would make the
  connection somewhere else). **Redirects are not followed** (`redirect::Policy::none()`). A development and test opt-in,
  `WEBHOOK_ALLOW_PRIVATE_TARGETS=true`, lets a local listener be the target (and allows `http`); the server logs a warning
  when it is on, and the exit-criterion test uses it, while other tests hold the strict policy against private targets
  without opening a connection.

### 5. The bank aggregator

- **A `BankAggregator` port** (ADR 0011 called it `BankConnector`; the word now belongs to the e-commerce packages) that
  starts a **connection** (the aggregator's own consent flow in the SPA gives back an id), lists what it can see, pulls the
  transactions of an account since a day, and disconnects. It has a fake adapter and **one real adapter written from the
  public documentation of a Brazilian Open Finance aggregator**; which one, and what the documentation does not say, is
  settled and said honestly in slice 4's ADR. **It cannot be validated without an account**; the adapter says so, as the
  fiscal ones do, and is never described as verified.
- **No secret per company is stored.** The aggregator's credentials are the installation's, in the environment (one
  account per installation, like the fiscal gateway); what is stored per company is a **connection record**: the bank
  account it feeds, the provider, the aggregator's connection id and the day of the last pull. A connection id alone
  gives nothing without the installation's credentials. An adapter that needed a token per company would store it through the
  `SecretCipher` and say so in its own ADR.
- **It feeds what already exists.** Transactions become statement lines through the **same import and fingerprint**
  ([ADR 0034](0034-statement-import-and-reconciliation.md)): the aggregator's transaction id takes the place of the bank
  reference, so a pull that overlaps the last one adds nothing, and reconciliation does not know where a line came from.
  A pull is a **`sync_bank_connection` job** per connection, queued by an hourly pass (the day is the worker's) and
  harmless to repeat. OFX stays and is the only path for a company without an aggregator.

### 6. E-commerce connectors

- **There are several, and a company turns them on in the settings.** Under **Configurações > Integrações** (the same
  place as the API keys, the webhooks and, later, the bank connections) a page lists the **catalogue of connectors** the
  product knows (a fixed list in the domain, WooCommerce first, each with a name, what it does, what it needs and a link
  to its README) and the company's **connectors**: a connector is a record of a kind, a name given by the person (several
  of one kind are allowed, one per store, at most 10), an **active** switch and the identifiers of what it uses. Only
  owners and admins reach it (the `members` area, with the keys and the subscriptions). Activating, deactivating and
  deleting are audited.
- **Activating makes only what any integrator could make by hand**: an **API key** with the scopes the kind needs, named
  after the connector, shown once, and a **webhook subscription** to the events the kind needs, whose secret the page
  shows (and edits, as for any subscription). The page then tells the person what to paste into the connector (the
  product's address, the key, the secret). **Deactivating revokes the key and switches the subscription off**; turning it
  on again makes a new key. The product therefore holds **no credential of the store** and never calls the store: the
  connector does, from where it runs, and its own settings (the store's address and keys) live in its own configuration.
  That is what keeps it a separate package and the activation honest: nothing the connector can do is outside `v1` and the
  webhooks.
- **A connector is a separate package** in `connectors/<kind>/`, with its own README, tests and lockfile, **importing no
  crate and no file of the workspace**: it speaks only the `v1` API with its key and receives only webhooks. The first is
  **WooCommerce** (justified in slice 5's ADR: it is open source and self-hosted like this product, its REST API and signed
  webhooks need no partner registration or OAuth app, whereas the largest Brazilian hosted platforms require one before
  any token exists). It is tested against a **local stand-in** and **cannot be validated against a real store without an
  account**. Another connector is a new package, one entry in the catalogue and, if it needs an event or a resource `v1`
  lacks, an **additive** change to `v1`.
- It maps an order to a sale with the order's id as the `external_reference`, so replaying an order is safe.

### 7. The PWA

- Installable (manifest, icons rendered from the logo, a service worker) with **offline read** of recent data and an
  offline indicator. **There are no offline writes**: a form that cannot reach the server says so. The cache rules are
  written as rules because a cache is another place a company's data lives:
  1. **An allowlist of what may be cached**, not a list of what may not (the allowlist is the real control: ordinary JSON
     carries no `Cache-Control`, so a `no-store` rule would hardly ever apply): only `GET` of the dashboard's sources and
     the lists the person opened (parties, products, entries, sales, quotes, stock positions, balances, cash flow). Never
     `/auth`, `/public`, `members`, `api-keys`, `webhook-subscriptions`, `accounting`, `fiscal`, attachments, exports,
     histories or payment links.
  2. **Per person and per company**: the worker cannot read the HttpOnly session cookie, so the **page tells it who is
     signed in** (the user id from `/auth/me`), the worker keeps that in IndexedDB (a worker is stopped and started at will)
     and names every data cache after the user and company; a different user empties the others before anything is read.
  3. **Cleared at sign-out and on any `401`, clearing first and calling the network second** (so signing out offline still
     clears), and an entry **expires** after 7 days even when nothing cleared it (a session that ended while the device was
     off). Personal data in a cached party (document, address) therefore stays on that device for at most that long or
     until sign-out: said on the page that turns the offline mode on, since it is the person's device and their call.
  4. **Bounded**: 1 MB a body, a few dozen entries; an entry read from the cache is marked,
     and the page shows "offline, data of {time}".
  5. The application's own files (hashed assets) are cached by name and are not data.
  The rules are tested in `web/` against a fake `caches` and `fetch`; slice 6's ADR holds the details.

## Consequences

- Integrators get a small contract that can be kept; the SPA is free to change. The price is a second set of DTOs for the
  `v1` resources and a document maintained by hand, held by three tests rather than by generation.
- A key cannot do anything its scopes do not name, cannot reach another company, never reads fiscal, accounting or members,
  and leaves its name in the trail.
- **Webhooks are an at-least-once hint, not the record.** A crash between a write's commit and its publish loses that event;
  a delivery may arrive twice; deliveries of different events may arrive out of order. The `v1` reads are the truth, and
  the documentation tells consumers to reconcile with them (`GET /v1/sales?updated_since=` and by `external_reference`).
  Moving to an outbox in the transaction would remove the first gap at the price of every write's signature; it is the
  first thing to revisit if the loss is ever seen.
- The rate limit is approximate across instances and forgotten on restart.
- Webhook secrets reuse the sealing of ADR 0085 (same key, no new dependency): none is ever at rest in plain text and the
  person can choose it, at the price of the same key to keep and the same lack of key rotation.
- `Actor` gaining a variant touches about two dozen places once, in slice 2, and the audit CHECK is widened by a new
  migration (the one already merged is not edited).
- `JobKind` gets a per-kind retry limit, and `MAX_ATTEMPTS` stops being a single number.
- Not validated, and said so in each slice: the aggregator adapter and the WooCommerce connector (no real account), and
  deliveries to a real subscriber over the internet (tests use a local listener with the private-target opt-in).

## Revisit when

- A real integrator needs an exact distributed rate limit, a per-key limit or a per-IP throttle.
- An event is seen lost: move events to an outbox written in the business transaction.
- A client needs real-time push beyond webhooks (server-sent events).
- The `SecretCipher` needs rotation, a key per company, a hardware-backed key or an external key service instead of one environment key.
- `v1` needs a breaking change: write `v2` and start the year.
