# conta-verde

An open-source ERP for small businesses, inspired by cloud ERPs such as Conta Azul.
Not affiliated with or endorsed by any commercial product.

> Status: early foundation. See the [roadmap](docs/roadmap.md) and the [architecture decisions](docs/adr).
> The API has no authentication yet, so do not expose it to untrusted networks.

## Quick start

Requirements: Rust (stable), Docker.

```bash
cp .env.example .env
docker compose up -d db
export $(cat .env | xargs)
cargo run -p conta-verde-api
```

Register and list a customer or supplier:

```bash
curl -X POST localhost:3000/companies/$(uuidgen)/parties \
  -H 'content-type: application/json' \
  -d '{"name":"Padaria Central","document":"11.222.333/0001-81","is_customer":true,"is_supplier":false}'
```

## Development

Install the git hooks once (needs Node and pnpm). The pre-commit hook runs the same checks as CI, except the database tests:

```bash
pnpm install
```

```bash
pnpm check                            # fmt, clippy and tests
cargo test --workspace -- --ignored   # needs the database from docker compose
```

The server listens on `127.0.0.1:3000` by default. Set `BIND_ADDR` to change it, and keep it private until authentication exists.

The code is organized in layers, one crate each: `domain`, `application`, `infrastructure`, `api`
([ADR 0002](docs/adr/0002-layered-cargo-workspace.md)).

## License

Licensed under the [GNU Affero General Public License v3.0 or later](LICENSE) (AGPL-3.0-or-later).
If you run a modified version as a network service, you must offer its source code to your users
([why AGPL](docs/adr/0001-license-agpl.md)).
