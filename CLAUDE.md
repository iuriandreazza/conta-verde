# conta-verde

Open-source ERP for small businesses (inspired by cloud ERPs such as Conta Azul): a Rust API (`crates/`) and a React SPA (`web/`).
License: **AGPL-3.0-or-later** (ADR 0001). Never use the Conta Azul name, logo or screenshots in code, docs or UI.

Read `docs/macro-plan.md` for scope (the reference module map and release plan), `docs/roadmap.md` for
phase order and progress, and `docs/adr/README.md` for the index of decisions. A phase starts by
accepting the `proposed` ADRs it depends on.

## Commands

```bash
pnpm install                 # once: installs husky and enables the pre-commit hook
pnpm check                   # Rust fmt/clippy/tests plus web typecheck/tests (what the hook runs)
docker compose up -d db      # PostgreSQL for local runs and ignored tests
export $(cat .env.example | xargs)
cargo test --workspace -- --ignored   # Postgres integration tests (need DATABASE_URL)
cargo run -p conta-verde-api
pnpm --filter conta-verde-web dev     # web app on http://localhost:5173, proxies /api to the API
```

CI (`.github/workflows/ci.yml`) runs the same checks plus the ignored tests against a Postgres service, and builds the web app.
The pre-commit hook does not run ignored tests, so run them yourself when touching `infrastructure`.

## Web app (ADR 0005)

- `web/` is Vite + React + TypeScript; it talks only to `/api/*`, which the Vite dev server (and a reverse proxy in production) forwards to the API with the prefix removed. Same origin means the session cookie works and no CORS is needed. Never add permissive CORS with credentials.
- `VITE_API_TARGET` points the dev proxy at the API (default `http://127.0.0.1:3000`). `VITE_SOURCE_URL` sets the footer's "Source code" link at build time; forks running modified code must point it at their own source (AGPL section 13).
- Keep the footer link and the API contract in `web/src/api.ts` in sync with the Rust DTOs by hand; there is no generated client yet.
- Tests are Vitest + Testing Library with `fetch` stubbed; prefer asserting what the user sees.
- Animation is Motion (`motion/react`) and icons are vendored Lucide Animated components in `web/src/components/icons/` (ADR 0016). Do not edit those by hand beyond the documented tweaks; to add one, download its registry JSON from `https://lucide-animated.com/r/<name>.json`, check it (only `motion/react`, `react` and `cn` imports, no network or DOM access) and replace the `@/lib/utils` import with `../../lib/utils`.
- A list page keeps its search, filters and order in one state object and gets its rows from `usePagedList` (`web/src/lib/usePagedList.ts`): it loads a first page again when the debounced values change, drops replies to an earlier request, and always sends a cursor with the same parameters that produced it. Use `ListRow` for rows and `History` (with a loader and labels) for a record's trail.
- Use `components/Button` for buttons (it plays its icon on hover/focus and shows a spinner while `pending`). Pages animate on enter only, and `Layout` remounts `<main>` on every path change, so page state never leaks across routes.
- The logo is `web/public/favicon.svg`, mirrored by `LogoMark` in `web/src/components/Logo.tsx`; change both together. The PNG and ICO favicons are rendered from the SVG.
- A browser pane or tab that is not painting never advances `requestAnimationFrame`, so animations look frozen at their first frame; that is the environment, not the app. Use a visible tab when checking motion.

## Architecture (ADR 0002)

One crate per layer; dependencies point inwards and the compiler enforces it.

| Crate | Holds | Depends on |
|-------|-------|------------|
| `domain` | Entities, value objects, rules. No I/O, no serde, no sqlx. | `uuid` only |
| `application` | Use cases (`*Service`) and port traits (`*Repository`). | `domain` |
| `infrastructure` | Adapters implementing ports: Postgres (sqlx) and in-memory. | `application`, `domain` |
| `api` | axum routes, request/response DTOs, `main.rs` wiring. | all |

Rules that are easy to break:
- `api` must not import `sqlx`; anything database-related lives in `infrastructure`.
- Adapter behavior is specified once in `crates/infrastructure/src/contract.rs` and run against both adapters; extend it when adding a port.
- Use cases are generic over their port (`PartyService<R: PartyRepository>`), not `dyn`. Port methods return `impl Future + Send`.
- Serde types belong in `api`. Domain types are not serialized directly.
- Every business table has `company_id`, every repository method takes a `CompanyId`, and uniqueness is scoped by company (ADR 0004).
- Every write to a business table carries an `AuditEntry` built by the use case (actor from `CompanyAccess`, field changes from `domain::audit`) and stored in the same transaction (ADR 0008). A new register needs its entity in `AuditEntity`, both adapters recording it, and a contract test that a refused write leaves no entry. Never audit account, session or membership writes, and never edit or delete `audit_log` rows (a trigger forbids it).
- List endpoints take `limit`, `after`, `q`, one closed-value parameter per filter and `sort` (`field` or `-field`), through a `*Query` type both adapters implement; unknown values are `400` JSON, never ignored (ADR 0018). Sort only by non-null columns ending in the id, so the keyset cursor stays valid. Bind search text with `strpos`, never `LIKE`.
- Money is exact decimal (`rust_decimal` / `NUMERIC`), never floats (ADR 0003). In `domain` it is `Amount`: strict parsing, never rounded, no trailing zeros; the API sends and takes decimal strings, and the web app never converts one to a `Number` (`web/src/lib/decimal.ts`).
- A new register copies the products one (ADR 0019): a domain type that validates and normalizes, a port with a contract test (scoping, atomic audit, refused writes leave no trace), an adapter pair, a `PUT` that replaces and audits only the difference, and a `CHECK` in the migration for every domain rule so the database refuses what the domain would. A value the domain accepts must be one Postgres accepts too (NUL bytes were the case that bit).
- SQL uses runtime `sqlx::query`, not `query!` macros. Do not add `query!` without also solving offline data for CI.
- A new adapter or port method needs both adapters (Postgres and in-memory) to behave the same, including ordering.

## Gotchas

- Auth is cookie sessions (ADR 0007). Every `/companies/{company_id}/...` handler must take the `CompanyAccess` extractor (before any `Json`/`Query`); missing sessions get `401`, non-members and unknown companies both get `404`.
- Not built yet: password reset, invitations, login rate limiting, roles. The API still binds to `127.0.0.1` by default and needs a TLS proxy plus `COOKIE_SECURE=true` before being exposed. Do not change the default bind address.
- The foreign key `parties.company_id -> companies` fails on old dev databases holding orphan parties: `docker compose down -v` resets it.
- Secrets (Argon2, token generation and digests) sit behind the `SecretService` port; the `argon2` crate is optimized even in dev builds (workspace `Cargo.toml`), or tests would take seconds per login.
- Migrations live in `crates/infrastructure/migrations/`. Do not edit one that has been merged to `main`; add a new one.
- Editing an applied migration locally breaks your dev database checksum: `docker compose down -v` resets it.
- Companies that have audit rows cannot be deleted in a dev database: `audit_log` references them and a trigger forbids deleting or updating its rows. Reset with `docker compose down -v` instead of `DELETE`.
- Storage errors are logged server-side and returned to clients as a generic `internal error`. Keep it that way.
- Documents are stored without punctuation and validated with check digits in `domain`: a CPF is 11 digits; a CNPJ is 14 characters whose first 12 may be uppercase letters (alphanumeric CNPJ, since July 2026). Never assume a document is numeric (no `inputMode="numeric"`, no digit-only parsing).
- A CNPJ is also looked up in the federal registry (ADR 0017); a CPF never is. Nothing the registry says blocks a registration: `NotFound` stores the party as `NotFound` with a warning (its data lags, so new companies can be missing), an outage stores it as `Unverified`, and `CNPJ_LOOKUP=off` (`Disabled`) stores it as `NotApplicable` without nagging. The port takes a `Cnpj`, which only `Document::cnpj` can build. Tests must never call the real service: use `InMemoryCnpjRegistry` or the local fake in `brasil_api.rs`. The real BrasilAPI answers 429 to requests without a `User-Agent`, and to callers that exceed its rate limit.

## Conventions

- Reply to the user in Brazilian Portuguese. Code, identifiers, commits, branch names and user-facing docs are in **English**.
- Commits and branches follow semantic prefixes (`feat`, `fix`, `docs`, `refactor`, `chore`, `test`), e.g. `feat/accounts-payable`.
- Comments explain why, not what. No dead code, no premature abstraction.
- Tests before refactoring. Run `pnpm check` before declaring work done.
- Docs under `docs/` start with frontmatter: `title`, `status`, `date`, `tags`. ADRs are `docs/adr/NNNN-slug.md`; record significant decisions as a new ADR.

## Workflow

- Work in a git worktree on a semantic branch, push with `gh`, open a PR to `main` (GitHub flow).
- **Review is local**: run `/code-review` on the branch and fix the findings before asking for merge. Do not wait for the Copilot reviewer.
- Documentation-only PRs (ADRs, `docs/`, CLAUDE.md) may be merged right after the push.
- Use `pnpm` for Node tooling and `uv` for Python, if either is ever needed.
