<p align="center">
  <img src="web/public/favicon.svg" alt="conta-verde logo" width="72" height="72">
</p>

# conta-verde

An open-source ERP for small businesses, inspired by cloud ERPs such as Conta Azul.
Not affiliated with or endorsed by any commercial product.

> Status: feature-complete for the planned scope and in the **1.0 hardening** phase (row-level security, backup and restore,
> importers, a performance baseline, user documentation). Registers, finance, sales, collection, purchases and stock, the
> accountant's tools, fiscal documents through a gateway, a public API with webhooks and a PWA are built; nothing has been
> tagged yet. See the [macro plan](docs/macro-plan.md) for scope, the [roadmap](docs/roadmap.md) for progress per phase,
> [ADR 0092](docs/adr/0092-one-point-zero-hardening-plan.md) for what 1.0 will promise and the
> [architecture decisions](docs/adr/README.md). The API binds to loopback by default and needs a TLS proxy
> before it is exposed.

## Quick start

Requirements: Rust (stable), Docker, Node and pnpm.

```bash
cp .env.example .env
docker compose up -d db
export $(cat .env | xargs)
cargo run -p conta-verde-api
```

In another terminal, start the web app (it proxies `/api` to the API) and open http://localhost:5173:

```bash
pnpm install
pnpm --filter conta-verde-web dev
```

Or start the database, a mail catcher, the API and the web app in one go with `pnpm dev`, which picks free ports when 5432, 3000, 5173, 1025 or 8025 are taken (and points the invitation links at the web port it picked)
(`pnpm db`, `pnpm api` and `pnpm web` run each one alone). Every e-mail the app sends (an invitation, for now) lands in
[Mailpit](https://mailpit.axllent.org), at the address `pnpm dev` prints, and never in a real mailbox.

Create an account in the browser, or from the command line (the session lives in the cookie jar):

```bash
curl -c jar -X POST localhost:3000/auth/register -H 'content-type: application/json' \
  -d '{"name":"Ana","email":"ana@example.com","password":"correct horse","company_name":"Padaria"}'
curl -b jar localhost:3000/auth/me   # shows your company id
curl -b jar -X POST localhost:3000/companies/<company id>/parties -H 'content-type: application/json' \
  -d '{"name":"Padaria Central","document":"11.222.333/0001-81","is_customer":true,"is_supplier":false}'
```

## Development

The pre-commit hook (installed by `pnpm install`) runs the same checks as CI, except the database tests:

```bash
pnpm check                            # Rust fmt, clippy and tests; web typecheck and tests
cargo test --workspace -- --ignored   # needs the database from docker compose
```

By default every CNPJ you register (never a CPF) is sent to [BrasilAPI](https://brasilapi.com.br), a free third-party
service, to confirm the company exists. Set `CNPJ_LOOKUP=off` to keep it all local; see
[ADR 0017](docs/adr/0017-cnpj-registry-lookup.md). The same service finds an address from its CEP, but only
when someone presses "Buscar endereço" in the party form; `CEP_LOOKUP=off` switches that off too
([ADR 0024](docs/adr/0024-postal-code-lookup.md)).

To send e-mail for real, set `SMTP_URL` and `SMTP_FROM` (see `.env.example`); without them messages are only written to the
log and the web app shows an invitation's link to pass on. A mail server on the internet needs TLS (`smtps://` or
`?tls=required`): a URL with a password that does not encrypt is refused unless the server is localhost, since the password
and the message would travel in the clear ([ADR 0014](docs/adr/0014-notifications.md)).

The server listens on `127.0.0.1:3000` by default. Set `BIND_ADDR` to change it, and keep it private
until it sits behind a TLS proxy (set `COOKIE_SECURE=true` there).

The code is organized in layers, one crate each: `domain`, `application`, `infrastructure`, `api`
([ADR 0002](docs/adr/0002-layered-cargo-workspace.md)). The web app lives in `web/`
([ADR 0005](docs/adr/0005-spa-frontend.md)).

## License

Licensed under the [GNU Affero General Public License v3.0 or later](LICENSE) (AGPL-3.0-or-later).
If you run a modified version as a network service, you must offer its source code to your users
([why AGPL](docs/adr/0001-license-agpl.md)).
