---
title: "ADR 0093: Row-level security as the second barrier"
status: accepted
date: 2026-11-11
tags: [adr, tenancy, security, postgres, phase-9]
---

# ADR 0093: Row-level security as the second barrier

## Context

[ADR 0004](0004-company-scoped-tenancy.md) promised, for Phase 9, PostgreSQL row-level security "keyed on a per-request session
variable as a second barrier, so that a missed `WHERE company_id` cannot leak rows". [ADR 0092](0092-one-point-zero-hardening-plan.md)
made it the first slice of the hardening and set its constraints. What the code is today:

- **One role, one pool.** The server connects as the role in `DATABASE_URL`, which in the compose file is the database's
  superuser and everywhere else owns the tables, and migrates with it at start. A superuser and a table's owner are not held to
  policies, so adding policies alone would change nothing.
- **About 300 call sites use the pool directly** (`.execute(&self.pool)`, `self.pool.begin()`) in 84 Postgres repositories. Every
  repository method takes a `CompanyId`, but the database never sees who the *request* is for; it sees only the id the use case
  chose to bind.
- **Work without a request** exists: eight background tasks in `main.rs` (the job worker, four webhook workers, the bank sync, the
  hourly passes for recurrences, contracts, reminders, the monthly notice, the fiscal poll), and public routes that find the company
  from a token (a quote's link, a payment page's link, an API key) or from the path of a webhook.
- 75 tables, 73 of which carry a `company_id`. Some tables are read **before** any company is known: who a person belongs to, what
  an invitation's link names, the session and the users.

## Options considered

### How the database learns which company a request acts for

| Option | Pros | Cons |
|--------|------|------|
| `SET LOCAL app.company_id` in a transaction opened by every repository method, from the method's own `CompanyId` argument | Exactly what ADR 0004 sketched; transaction-scoped | About 300 call sites change, and the 216 single statements would become transactions (a `BEGIN` and a `COMMIT` more each). Worse, the scope would come from the *argument* the use case chose: a handler that passes another company's id would set that company as the scope and the barrier would let it through. It guards against a forgotten `WHERE`, not against a wrong id |
| A scope set once at the edge, from what was **verified** (the caller's membership, the API key, the token), carried to the connection by the pool (chosen) | Small change (the edge, the pool's hooks, the workers); the scope comes from the authentication, so a repository asked for another company's rows finds none; a request that forgot to authenticate finds none | The scope travels in a task-local, which `tokio::spawn` forgets (two places needed it, listed below); a connection setting is state, so it must be set on **every** acquire |
| One database role per company | Real isolation | Roles cluster-wide for every company, a pool per company; not a thing a small self-hosted install wants |

### How work that belongs to no company is allowed through

| Option | Pros | Cons |
|--------|------|------|
| A second `OR` in every policy that lets a flag through | One policy | The planner cannot use an index for `company_id = x OR stable_function()`: **every request's every query would become a sequential scan**, to serve eight background tasks |
| `SECURITY DEFINER` functions for each cross-company read | Narrow | One per read (and the passes then still need the company for what follows); each is code that runs as the owner |
| A second role with `BYPASSRLS` and `SET ROLE` | A real bypass | The operator must create it; the runtime role could always switch to it; a second pool or a switch at each acquire |
| The scope is a **list** of companies, and background work gets the list of every company that exists (chosen) | One policy, indexable (`= ANY(array)`); no second role | The list is read at each acquire (it is a scan of `companies`, one row per company) and parsed by each query: a cost the passes pay and requests never do; no isolation for background work, by design |

### Who may be the runtime role

| Option | Pros | Cons |
|--------|------|------|
| `FORCE ROW LEVEL SECURITY` and let the owner serve requests | One role to configure | `pg_dump` run as the owner would then be filtered by the policies, so a backup would silently be empty or fail, and every data migration would need a bypass; a superuser still escapes |
| A role that **owns nothing and has no `BYPASSRLS`**, with the owner migrating (chosen) | The standard shape; backups and migrations as the owner see everything | Two URLs to configure (`DATABASE_MIGRATE_URL` is new); a deployment that has one role must make a second |

## Decision

### The scope

- Every connection carries the session setting `app.company_ids`, a comma-separated list of company ids. The function
  `cv_company_ids()` (`STABLE`, so an index can be used) reads it, and **every table with a `company_id` has one policy**,
  `tenant_isolation`, `FOR ALL`, with `USING` and `WITH CHECK` both `company_id = ANY (cv_company_ids())`: a row of another
  company is invisible, cannot be updated or deleted, and cannot be inserted (or moved into) with the company's id changed. An
  empty setting matches nothing, so **no scope is no access**. Policies are `ENABLE`d, not `FORCE`d: the owner (migrations,
  backups) is unaffected.
- The scope lives in a task-local cell (`infrastructure::tenant`). The pool is built with `after_connect` and `before_acquire`
  hooks that write the **current task's** scope to the connection every time one is handed out (sqlx calls both inline in the
  acquiring task, which a test holds). A connection therefore never keeps the scope of its previous user.
  *We set a session value at acquire, not `SET LOCAL` in a transaction*: it needs no transaction around the 216 single
  statements and gives the same guarantee, since it is overwritten at every acquire.
- **A request** gets an empty cell from a layer around the whole router. `CompanyAccess` fills it with the route's company
  **after** `authorize` has proved the caller is a member; the `v1` gate gets it from the key; the three lookups by token (an
  API key's digest, a quote's link, a payment page's link) run in the system scope, find the company the secret names, and
  narrow the request to it. The public webhooks set it from the company in their path, which is a claim until the signature is
  checked and can only ever show that company. A handler that does none of this sees nothing and its writes are refused.
- **Background work** (the eight tasks above) runs in the **system scope**: the list of every company. The barrier does not
  protect it, and nothing here pretends otherwise; it is the application's own checks, as before. `tokio::spawn` inside a request
  forgets the scope: the one place that does (the fiscal webhook's background poll) starts it with `tenant::within(company, ...)`.
- Tables **left out**, because they are read before any company is known: `memberships` (which companies a person may open) and
  `invitations` (found by the digest of the link or the address invited). The tables without a `company_id` (`companies`,
  `users`, `sessions`, the tokens of password resets and e-mail confirmations, `jobs`, which is operational and has no company)
  are outside by construction. The list is the constant `LEFT_OUT` of `postgres_tenancy.rs`, each with its reason.

### The roles

- `DATABASE_URL` is the role that serves requests: it must own nothing and have no `BYPASSRLS`. **The server checks at start and
  refuses to run as a superuser, a `BYPASSRLS` role or an owner of the tables**, with a message that says what to do;
  `ALLOW_PRIVILEGED_DATABASE_ROLE=true` runs anyway, with a warning that the second barrier is off (the one-role development
  environment uses it, `deploy/entrypoint.sh`).
- `DATABASE_MIGRATE_URL` (optional, defaults to `DATABASE_URL`, which then fails the check) is the role that owns the schema. At
  every start the server connects with it, takes an advisory lock (two servers starting together would otherwise collide on the
  grants), migrates, then **grants the runtime role** `SELECT, INSERT, UPDATE, DELETE` on the tables and the sequences, and
  `REVOKE`s `UPDATE` and `DELETE` on `audit_log` (a second lock beside its trigger) and everything on `_sqlx_migrations`. Granting at
  every start means a table a later migration adds needs no grant of its own.
- The role is created by the operator, once: `deploy/postgres/create-app-role.sql` (the compose file runs it when the database
  is first made; CI runs it as a step; the operations guide of the next slice says it for production). A database that already
  exists needs the file run by hand.
- Tests and CI run as the runtime role. The adapters' contract tests, which have no request around them, are given the
  **system scope** by their pool (`contract::test_pool`) and so run the whole existing suite through the policies; the barrier has
  tests of its own, with a narrow scope. A few tests that change the schema (a trigger) or check what the constraints refuse use
  `contract::owner_pool`, because a policy refuses a row of another company before a constraint can.

### What a migration must do

A migration that adds a table with a `company_id` calls `SELECT cv_enable_tenancy('the_table')`. A test (`every_table_with_a_company_has_a_policy`)
lists every such table from the catalogue and fails when one has no row security and no policy, unless it is in `LEFT_OUT`.

### What it does and does not protect

- **It protects** against a query that forgets `WHERE company_id` or builds it wrong, against an id from the path or the body that
  names another company's row, and against a write that would put a row into another company: all of them find nothing or are
  refused with `42501`. It makes a handler that forgot to check the caller fail closed.
- **It does not protect** the background work (system scope); a request whose caller *is* a member of the company it names (that is
  authorization, the first barrier); the identity tables; a statement that runs as the owner or a superuser; SQL injection
  (every statement binds its values, and a session that can run arbitrary SQL can set its own scope).
- A foreign key's own checks run as the table's owner and are not filtered, so a composite foreign key still enforces what it did.

## Proof

`postgres_tenancy.rs` (Postgres tests, run in CI as the runtime role):
`a_query_for_another_companys_rows_finds_nothing_and_a_write_is_refused` asks a repository for another company's party by its id
(the application's check is skipped, the `WHERE` accepts the id) and gets nothing, counts zero, updates and deletes zero rows, has an
insert into the other company and a move of its own row into it refused with `42501`, and leaves the other company's row intact;
`work_that_was_never_narrowed_sees_nothing_and_system_work_sees_every_company`;
`a_connection_never_keeps_the_scope_of_whoever_used_it_before` (one connection, alternating scopes);
`concurrent_requests_each_see_only_their_own_company` (twelve tasks on three connections);
`every_table_with_a_company_has_a_policy`; `the_server_does_not_serve_requests_as_a_role_the_policies_do_not_bind`.
Through the real API and Postgres as the runtime role: `scripts/e2e-accountant.py`, `scripts/e2e-woocommerce.py` (an API key,
sales, signed webhooks delivered by the workers) and the new `scripts/e2e-areas.py` (a purchase, stock, a sale, a quote answered
through its public link, a charge and its public page, the reports, the job queue, and a second company that sees none of it).

## Cost

Measured with `pgbench` on a database of 10 companies with 50,000 entries each (500,000 entries, about 750,000 installments), four
clients, eight seconds each, PostgreSQL 17 in Docker on a laptop; the owner pays no policy, the runtime role pays it:

| Statement (one company of ten) | Owner, no policy | Runtime role, policy | Difference |
|---|---|---|---|
| One entry by its id | 0.068 ms | 0.109 ms | +0.04 ms |
| A keyset page of 50 entries (company, competence date, id) | 0.105 ms | 0.211 ms | +0.11 ms |
| A report: the year's receivable installments summed by category (joins 750,000 installments to the entries) | 63.4 ms | 48.8 ms | within noise (the runtime run came second, on a warm cache; it is not a speed-up) |
| `SELECT set_config('app.company_ids', ...)` against `SELECT 1` | 0.020 ms | 0.020 ms | none |

The policy is inlined into the index condition (`company_id = ANY (...)` beside the query's own `company_id = ...`), so every plan
keeps its index scan; what it adds is a fixed cost per statement of about 0.04 to 0.1 ms, planning and evaluating `cv_company_ids()`
(the plan of the page above shows 0.5 ms of planning). A statement that takes milliseconds does not notice; a point read doubles in
a time nobody sees. The report shows no cost, because its time is spent reading rows, which the policy does not touch.

The round trip that sets the scope at each acquire is the second cost: as cheap as an empty statement (the last row), and a
request pays it once per repository call, since each acquires a connection. **Not measured here**: the system scope's list of
companies with thousands of companies, and the whole API under load; the slice-5 baseline carries the policy and will show both.

## Consequences

- **Deployment changes** (and 0.x databases are not supported past this, ADR 0092): a runtime role and a second URL. The compose
  file, `.env.example`, `pnpm dev` and CI are updated. An operator who keeps one role must say so with
  `ALLOW_PRIVILEGED_DATABASE_ROLE=true` and has no second barrier.
- **A new background task must be started with `spawn_system`**, or it sees nothing; a new `tokio::spawn` inside a request must
  `tenant::within(company, ...)` (a spawned task forgets the scope). The failure of forgetting is closed (empty reads, refused writes), not a leak.
- **A token that names a company** is a lookup in the system scope followed by `tenant::set_company`; a new kind of token does the
  same in its repository.
- The system scope reads the list of companies at every acquire and each query parses it. For an installation with thousands of
  companies the passes pay for that; a bypass role or a per-company loop would be the next step.
- `pg_dump` runs as the owner (the backup guide of the next slice says so); a backup run as the runtime role would back up nothing.

## Revisit when

An installation has so many companies that the system scope's list is slow; a background task needs the barrier (it would loop over
companies and run each in its own scope); or a tenant needs physical isolation ([ADR 0004](0004-company-scoped-tenancy.md)).
