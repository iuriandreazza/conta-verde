---
title: Configuration
status: living
date: 2026-11-11
tags: [operations, configuration]
---

# Configuration

Everything is configured by environment variables, read once at start. A name, its accepted values and its meaning are kept through 1.x
([ADR 0092](../adr/0092-one-point-zero-hardening-plan.md)); a variable may be deprecated in a minor (the server logs a warning) and removed only in the next major.
`.env.example` at the root is the same list for development.

## The database and the server

| Variable | Meaning | Default |
|----------|---------|---------|
| `DATABASE_URL` | The role the server **serves requests as** (`postgres://conta_verde_app:...@host/db`). It must own nothing and have no `BYPASSRLS` ([ADR 0093](../adr/0093-row-level-security.md)) | required |
| `DATABASE_MIGRATE_URL` | The role that **owns the schema**: it migrates at start and grants the role above its rights | `DATABASE_URL` (which then fails the check below) |
| `ALLOW_PRIVILEGED_DATABASE_ROLE` | `true` runs even when `DATABASE_URL`'s role owns the tables, is a superuser or bypasses policies, **with no second barrier**. For development only | unset: refuse |
| `BIND_ADDR` | Where the API listens. The image sets `127.0.0.1:3000` inside the container; do not expose the API itself | `127.0.0.1:3000` |
| `PUBLIC_URL` | Where people open the app: the links in e-mails (invitations, quotes, payment pages) are made from it | `http://localhost:5173` |
| `COOKIE_SECURE` | `true` when the app is served over HTTPS, so the session cookie is only sent securely | `false` |
| `ATTACHMENTS_DIR` | Where the files attached to entries and the fiscal files are kept: **the only copy**, back it up | `./data/attachments` (`/data/attachments` in the image) |
| `SOURCE_URL` | The link to the source in the footer of e-mails (AGPL section 13): a fork running modified code points it at its own | upstream repository |
| `RUST_LOG` | Log level (`info`, `debug`, or a filter) | `info` |

## Sealed secrets

| Variable | Meaning |
|----------|---------|
| `FISCAL_SECRETS_KEY` | 64 hexadecimal characters. Seals a company's gateway settings and webhook secrets. **Without it those features are off, and if it is lost what it sealed cannot be opened.** Keep it apart from the backups |

## E-mail and messages

| Variable | Meaning |
|----------|---------|
| `SMTP_URL`, `SMTP_FROM` | Both or neither. `smtps://user:pass@host:465` or `smtp://user:pass@host:587?tls=required`. Without them nothing is sent and invitation links are shown on screen |
| `WHATSAPP_PHONE_NUMBER_ID`, `WHATSAPP_TOKEN`, `WHATSAPP_TEMPLATE` | All three or none: reminders through the WhatsApp Business Cloud API |

## What the server asks other services

| Variable | Meaning | Default |
|----------|---------|---------|
| `CNPJ_LOOKUP` | `brasilapi` sends each CNPJ you register (never a CPF) to BrasilAPI to confirm the company exists; `off` keeps it local | `brasilapi` |
| `CEP_LOOKUP` | `brasilapi` looks up an address from a CEP when the person asks; `off` disables it | `brasilapi` |
| `PAYMENT_PROVIDER` | `off`, `asaas` or `stripe` (`fake` is for development only). Real providers read the variables below; the server refuses to start with one missing | `off` |
| `ASAAS_API_KEY`, `ASAAS_WEBHOOK_TOKEN`, `ASAAS_API_URL` | Asaas credentials (`ASAAS_WEBHOOK_TOKEN` 32 to 255 characters); the URL is optional (the sandbox) | |
| `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET`, `STRIPE_API_URL` | Stripe credentials (`whsec_...`); the URL is optional | |
| `FISCAL_GATEWAY` | `off`, `focus` or `nfeio` (`fake` is for development only): the gateway that issues fiscal documents. A company may store its own settings instead | `off` |
| `FOCUS_NFE_TOKEN_HOMOLOGATION`, `FOCUS_NFE_TOKEN_PRODUCTION`, `FOCUS_NFE_ISSUER_CNPJ`, `FOCUS_NFE_API_URL` | Focus NFe | |
| `NFEIO_API_KEY`, `NFEIO_COMPANY_ID_HOMOLOGATION`, `NFEIO_COMPANY_ID_PRODUCTION`, `NFEIO_ISSUER_CNPJ`, `NFEIO_API_URL`, `NFEIO_FILE_HOSTS`, `NFEIO_SEARCH_CONFIRMED` | NFE.io (production issues are refused until `NFEIO_SEARCH_CONFIRMED=true`, which you set only after the sandbox test) | |
| `FISCAL_WEBHOOK_SECRET`, `FISCAL_WEBHOOK_HEADER` | The secret (32 characters or more) and the header the gateway's notices carry | |
| `BANK_AGGREGATOR` | `off` or `pluggy` (`PLUGGY_CLIENT_ID`, `PLUGGY_CLIENT_SECRET`, optional `PLUGGY_API_URL`) | `off` |

The adapters of the payment providers, the fiscal gateways and the bank aggregator are written from the services' documentation and **not exercised
against the real services** by the maintainers; their ADRs say what was checked.

## The public API and webhooks

| Variable | Meaning | Default |
|----------|---------|---------|
| `API_RATE_LIMIT_PER_MINUTE` | Requests a `v1` API key may make a minute. Counted in each process: with N instances the real limit is N times this | `300` |
| `WEBHOOK_ALLOW_PRIVATE_TARGETS` | `true` lets a webhook go to `http` and private addresses. Development and tests only: never in production | unset |

## Development only

`FAKE_PAYMENT_WEBHOOK_SECRET` (what the fake provider's notices are signed with) and the `fake` values above make things that have no value. The image's web
server also needs nothing set: it listens on 3080 and forwards `/api` to the API.
