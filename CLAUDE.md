# conta-verde

Open-source ERP for small businesses (inspired by cloud ERPs such as Conta Azul), written in Rust.
License: **AGPL-3.0-or-later** (ADR 0001). Never use the Conta Azul name, logo or screenshots in code, docs or UI.

Read `docs/roadmap.md` for scope and phase order, and `docs/adr/` for the reasons behind decisions.

## Commands

```bash
pnpm install                 # once: installs husky and enables the pre-commit hook
pnpm check                   # fmt --check, clippy -D warnings, tests (what the hook runs)
docker compose up -d db      # PostgreSQL for local runs and ignored tests
export $(cat .env.example | xargs)
cargo test --workspace -- --ignored   # Postgres integration tests (need DATABASE_URL)
cargo run -p conta-verde-api
```

CI (`.github/workflows/ci.yml`) runs the same checks plus the ignored tests against a Postgres service.
The pre-commit hook does not run ignored tests, so run them yourself when touching `infrastructure`.

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
- Money is exact decimal (`rust_decimal` / `NUMERIC`), never floats (ADR 0003).
- SQL uses runtime `sqlx::query`, not `query!` macros. Do not add `query!` without also solving offline data for CI.
- A new adapter or port method needs both adapters (Postgres and in-memory) to behave the same, including ordering.

## Gotchas

- Auth is cookie sessions (ADR 0007). Every `/companies/{company_id}/...` handler must take the `CompanyAccess` extractor (before any `Json`/`Query`); missing sessions get `401`, non-members and unknown companies both get `404`.
- Not built yet: password reset, invitations, login rate limiting, roles. The API still binds to `127.0.0.1` by default and needs a TLS proxy plus `COOKIE_SECURE=true` before being exposed. Do not change the default bind address.
- The foreign key `parties.company_id -> companies` fails on old dev databases holding orphan parties: `docker compose down -v` resets it.
- Secrets (Argon2, token generation and digests) sit behind the `SecretService` port; the `argon2` crate is optimized even in dev builds (workspace `Cargo.toml`), or tests would take seconds per login.
- Migrations live in `crates/infrastructure/migrations/`. Do not edit one that has been merged to `main`; add a new one.
- Editing an applied migration locally breaks your dev database checksum: `docker compose down -v` resets it.
- Storage errors are logged server-side and returned to clients as a generic `internal error`. Keep it that way.
- Documents (CPF/CNPJ) are stored as digits only and validated with check digits in `domain`.

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
