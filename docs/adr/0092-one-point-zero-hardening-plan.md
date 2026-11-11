---
title: "ADR 0092: What 1.0 means, and the plan for Phase 9 (hardening)"
status: accepted
date: 2026-11-11
tags: [adr, release, hardening, phase-9]
---

# ADR 0092: What 1.0 means, and the plan for Phase 9 (hardening)

## Context

Phases 0 to 8 are merged. Phase 9 has five roadmap items: row-level security, backup and restore with a published image
and an upgrade guide, importers for leaving another product, a performance baseline with an index review, and complete
user documentation with a Portuguese UI. Its exit criterion is "an operator upgrades two consecutive versions in place
with a restored backup and all checks green". Three facts shape the plan:

- **No version has ever been tagged.** `git tag` is empty; "v0.3" and the like in the roadmap are names of phases, not
  releases. So there is no released version anyone can be upgrading from, and "1.0" is not a number on a milestone but the
  first moment the project **promises something to people who did not write it**. What it promises must be written
  before it is promised.
- **The database is migrated by the server itself at start** (`connect_and_migrate`, one `DATABASE_URL`, one role that
  owns the tables and also serves every request). That is the opposite of what row-level security needs (a runtime role
  that owns nothing), so the item that looks like "add policies" is also a change in how the server is deployed.
- **The pieces of operation exist only for development**: a `Dockerfile` and `deploy/nginx.conf` for the dev
  environment, a compose file with a superuser database, an attachments directory the docs already say "holds the only
  copy". Nothing says how to back them up together, upgrade, or roll back.

Principle 3 of the [macro plan](../macro-plan.md) and [ADR 0004](0004-company-scoped-tenancy.md) already promised the first
item ("Phase 9: PostgreSQL row-level security … as a second barrier"). This ADR plans the phase; each slice that follows
writes its own ADR with the design it settles (numbers are taken when they are written, from a fresh `main`).

## Decision

### What 1.0 promises

A promise is only worth making if a test or a script can break on it. Each line says where that happens.

**1. The public API `/v1` is kept for the whole of 1.x.** [ADR 0087](0087-public-api-v1.md) already makes `v1` a curated,
frozen set of resources: the document must contain `docs/api/v1/openapi.frozen.json` (nothing removed or renamed, no type
changed, no request field newly required), enforced by `v1/tests.rs`. 1.0 adds only the time promise: a break is a `/v2`
served **next to** `v1`, and `v1` is kept for at least **12 months after `v2` is released** and then removed in a
release that says so. The same holds for what `v1` sends out: the webhook body, event types and the
`Webhook-Signature` scheme ([ADR 0088](0088-webhooks.md)) are part of `v1`. The SPA's own routes (`/companies/{id}/...`
without `/v1`) are **not** a contract and may change in any release; the cookie session is for the SPA only.

**2. Migrations are forward-only, and an upgrade is in place.**

- A migration merged to `main` is never edited (already a rule); a mistake is fixed by a new migration. There are no
  down migrations and none will be written.
- A database made by any `1.x.y` is upgraded by starting any later `1.x'.y'` against it: the server applies the pending
  migrations at start, in order, in a transaction each (sqlx), and refuses to start if the database holds a migration it
  does not know (a newer database than the binary) or one whose checksum changed.
- **The only supported way back is to restore the backup taken before the upgrade.** The guide says so first. A release may
  not promise that an older binary runs on a newer schema.
- Because no version was ever tagged, **databases from before the first tag are not supported**: they were development
  databases, and some were reset with `docker compose down -v` more than once. The first tagged release is the first
  boundary the promise protects. (The importers of slice 4 are how data from *another product* comes in.)

**3. Configuration names keep their meaning.** Every variable in `.env.example` (and the ones the API reads in `main.rs`)
keeps its name, its accepted values and its default meaning through 1.x. New variables may be added (off or inert unless
set). A variable may be **deprecated** in one minor (the server logs a warning at start naming the replacement) and removed
in the next. A default that makes the server safer is never loosened: the default bind address stays `127.0.0.1` (the image
sets its own inside the container), `COOKIE_SECURE`, `CNPJ_LOOKUP` and the others behave as documented. The secrets that seal
stored data (`FISCAL_SECRETS_KEY`) keep their format; a key that must change gets a documented rotation, never a silent one.

**4. File layouts that other software reads are fixed.** The accountant's exports ([ADR 0071](0071-accountant-exports.md)),
the fiscal files in the package ([ADR 0076](0076-nfe-documents.md)), and the headers the importers accept
([ADR 0025](0025-csv-import.md), [ADR 0064](0064-entry-import-from-a-file.md) and the stock importer of this phase) change only
by a new ADR, and an importer keeps accepting what it accepted.

**5. What is *not* promised.** The Rust crates' APIs and the database schema as an integration surface (read it, but it is
ours to change; use `/v1` or the exports); the SPA's routes and DTOs; the text of an error outside `v1`; the exact timing of
the hourly passes; anything that needs a real external account and was never exercised (the Pluggy, Focus NFe and NFE.io
adapters, the WooCommerce connector: each ADR says so and the user docs repeat it); the **tax calculation** for any regime but
the Simples Nacional, which stays refused until a human accountant signs
[`docs/fiscal/accountant-checklist.md`](../fiscal/accountant-checklist.md) ([ADR 0084](0084-tax-calculation-behind-a-port.md)).

### The slices, in order, and how each is proven

One slice per pull request, each starting from a fresh `main`. The order is the dependency order; a reason is given where
it matters.

| # | Slice | Proved by | What the proof does **not** cover |
|---|-------|-----------|-----------------------------------|
| 1 | This ADR, the roadmap's stale phase titles, the README status | Review (documentation only) | — |
| 2 | **Row-level security** | A test with the application's company check deliberately skipped (a test-only repository call that names another company) finds no rows and has a write refused; a CI guard that lists every table with a `company_id` from `information_schema` and fails when one has no policy or has RLS off; the whole existing suite runs as the non-owner runtime role; the cost is **measured** and reported | A table added later without a policy is caught by the guard only if it has a `company_id` column; a bug in the policy expression itself is covered by the barrier test on the tables it names, not on all |
| 3 | **Operations**: image, proxy example, backup, restore, upgrade guide | A script that dumps a seeded database and its blobs, restores them into a fresh database and directory, runs a smoke check (the API starts, signs in, reads the seeded records and the files); a rehearsal script that builds data with an older ref, upgrades to the newer one in place, compares counts and a fingerprint of the books, then restores the pre-upgrade backup and compares again; a CI job that builds the image without pushing it; a workflow that publishes only on `v*` tags and needs only `GITHUB_TOKEN` | The **publishing** workflow cannot be run before a tag exists, and no secret-free test can push to a registry: it is reviewed, not run. The rehearsal needs two refs with different schemas; until a tag exists it uses commits (documented). Real hosts, TLS certificates and a managed Postgres are not rehearsed |
| 4 | **Importers** for stock balances and the guide for leaving a reference product | The pipeline contract of [ADR 0025](0025-csv-import.md): every row checked before any write, a refused row refuses the file, audited writes, sending twice adds nothing (idempotent by SKU and a movement reference); contract tests on both adapters and the stock tests of [ADR 0054](0054-stock-movements.md) still hold (balance and average cost after an import equal the worked example) | The guide describes the steps from a reference product's export as the maintainers understand them; it is not tested against a real account of any vendor |
| 5 | **Performance baseline** and the index review | A deterministic seeding tool (a script, never in the app binary) and a benchmark script that run against Postgres with row-level security **on**; `EXPLAIN (ANALYZE, BUFFERS)` kept for the slow queries; a migration only for an index a recorded plan justifies, with the query beside it | One machine, one Postgres version, synthetic data: a baseline to compare against, not a service level. It does not measure concurrent load |
| 6 | **User documentation and the Portuguese UI**, in several pull requests | A test that every key exists in both catalogues, one that no rendered page shows a raw key, and the existing web tests under both languages; screenshots taken from this app only | Documentation correctness is read by a person against the running app, not by a test; the Portuguese catalogue is checked for presence, not for quality |

Why this order: **2 before 3**, because restoring must restore the roles' grants and the policies, and the guide describes the
deployment RLS forces (an owner role that migrates and a runtime role that serves); **2 before 5**, so the baseline carries
the cost of the barrier instead of hiding it; **4** depends on nothing and could move, but it is cheaper after the guide
exists (the guide links to it); **6 last** because the screenshots and the documentation should describe the final UI, and
because it is the largest. If a slice finds that an earlier one was wrong, it fixes it in its own pull request and says so.

### What each slice has already been told it must settle

These are constraints from this plan, not the designs; each design is written, with its alternatives, in the slice's own ADR.

- **RLS.** The runtime role must **not own the tables and must not have `BYPASSRLS`**, and the server checks this at start
  (a superuser or a table owner defeats the barrier, so a deployment that would is refused or warned loudly, as the design
  decides). `DATABASE_URL` keeps its meaning (the runtime role); a separate, optional URL for the role that migrates is
  new. The company is set per transaction (`SET LOCAL`), so the one place every repository call opens a transaction or takes
  a connection has to be found or made. Work that has no company (the job queue, the hourly passes that visit every
  company, webhook and public routes that find the company from a token, login) is decided **explicitly**, with the smallest
  bypass that works, and listed in the ADR. Tables without a `company_id` (`users`, `sessions`, `companies`, memberships'
  lookups and the like) are listed with the reason each is left out. The development database and CI use the role split too,
  so a test never runs with a barrier that production does not have.
- **Operations.** The image is multi-stage, runs as a non-root user and is small; the proxy example sets
  `COOKIE_SECURE=true` and rate-limits the sign-in route (the API has **no login rate limiting**, stated in
  `CLAUDE.md`; the proxy is the documented answer for 1.0, and moving it into the API is out of scope); the backup holds the
  database, `ATTACHMENTS_DIR` (which includes the fiscal files) **taken consistently together**, and says that the
  environment secrets (above all `FISCAL_SECRETS_KEY`, without which sealed settings cannot be opened) are **not in the dump** and
  must be kept separately.
- **Importers.** Never names a product, shows its logo or copies its text; the guide says "a reference product's export", like
  `scripts/export-to-entries-csv.py`. The entries import already exists and is not redone.
- **Performance.** The provisional budget the baseline measures against, to be revised in the baseline document if the data
  says the number was wrong: on the 5-year dataset (per large company about 100 thousand entries with their installments,
  100 thousand settlements, 60 thousand sales with their lines, 200 thousand stock movements and a million audit rows, plus a
  few small companies so the planner sees other tenants) a **keyset list page ≤ 200 ms**, **every report, the DRE, the cash
  flow and each dashboard card ≤ 1.5 s**, measured on the API and on a laptop-class machine, p95 over repeated runs. A query
  over the budget is listed in the baseline with its plan, fixed if the fix is an index or a rewrite that is cheap, and
  otherwise left written down.
- **Language.** The **default is Portuguese (pt-BR)**: every person who uses the app today sees Portuguese, the product serves
  Brazilian small businesses, and the existing texts become the Portuguese catalogue without being retyped. The source keys are
  English. English is chosen from a switch, remembered **per person**; the way it is remembered (a stored preference on the
  account, or the device) and whether the browser's language is a first guess are settled in the i18n slice's ADR, with a
  small home-made layer preferred to a dependency unless that ADR justifies one. The user documentation is in English (the
  repository's convention for user docs), and the Portuguese UI is the one the people it describes see.

### Out of scope for 1.0

Written here so nobody reads their absence as an oversight: a per-database tenancy option (ADR 0004's "revisit when" stands),
SSO, login and API rate limiting shared between instances (the `v1` limit stays per process,
[ADR 0087](0087-public-api-v1.md)), streaming the accountant's package ([ADR 0071](0071-accountant-exports.md)), moving the
period lock's check into the write's transaction ([ADR 0069](0069-period-lock.md)), handing a company over to another owner,
native apps, a languages catalogue beyond Portuguese and English, and any statutory fiscal calculation. Each stays recorded in
the ADR that decided it; none blocks the tag, and the release notes list them.

### What the tag still needs after the five items

The slices make 1.0 *possible*; they do not cut it. What is left for a person: choosing the version and tagging it (the image
is published by the tag, never by a pull request), reading the publishing workflow once against the real registry settings,
the accountant's signature if the regular regimes are to be turned on, and homologation of the fiscal and bank adapters
against real accounts (their ADRs list what to check). The roadmap ticks an item only for what a test or a document proves;
the rest is written beside the item.

## Consequences

- A release is no longer a phase boundary: the macro plan's table of versions 0.1 to 0.9 stays as the **names of phases** (it
  says so now), and the first tag is the first version number that means something to an operator.
- The roadmap's phase titles stop saying "(v0.N)" without a state: each says whether the phase is done, so a reader is not
  told that a finished phase is open.
- `CLAUDE.md` gains a rule for each promise as its slice lands, so a change that would break one meets it in the
  instructions before it meets a user.
- Writing "no down migrations" and "restore is the way back" commits the project to taking the backup step seriously in the
  upgrade guide and to keeping a rehearsal in CI that proves a restore works.

## Revisit when

A second `/v1` resource family is requested that cannot be added without breaking the frozen copy (the `v2` rule is then used
for the first time); an operator needs to run an older binary on a newer schema (a compatibility window would then need an
expand/contract rule for migrations); or the first real upgrade between two tags shows that the rehearsal missed a step.
