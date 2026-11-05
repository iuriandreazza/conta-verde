---
title: "ADR 0017: CNPJ checked against the federal registry behind a port"
status: accepted
date: 2026-11-05
tags: [adr, registers, integrations]
---

# ADR 0017: CNPJ checked against the federal registry behind a port

## Context

A CNPJ with valid check digits can still be a typo for another company, or a company that does not exist. Receita Federal
keeps the registry, but the only official programmatic lookup (Serpro's Consulta CNPJ) is a paid contract, and the free
public page sits behind a captcha. Free services that republish Receita's open data exist, run by third parties.
A CPF has no free lookup, and querying people's CPFs against a third party is a privacy problem we do not want.

Since July 2026 Receita also issues **alphanumeric CNPJs**: 14 positions, the first 12 digits or uppercase letters,
the last two numeric check digits (modulo 11, each character valued as its ASCII code minus 48). Numeric CNPJs stay valid.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Check digits only | No network, no third party | Accepts typos that land on valid numbers and companies that never existed |
| Serpro Consulta CNPJ (official) | Government source | Paid, credentials per installation, could not be tried without a contract |
| Free community API (BrasilAPI) | No signup, understands alphanumeric CNPJs, returns legal name and situation | No uptime guarantee, strict rate limit, data can lag, third party sees the CNPJ |
| Lookup behind a port, BrasilAPI first (chosen) | Operators can swap or switch it off; the domain never learns who answers | One more adapter and a failure mode to handle |

## Decision

- `domain`: `Document` accepts both CNPJ formats and a CPF (digits only, check digits only). `DocumentCheck` records what
  is known about a party's document: `NotApplicable` (no document or a CPF), `Verified { legal_name, situation }`,
  or `Unverified`.
- `application`: the `CnpjRegistry` port answers `RegistryEntry`, `NotFound` or `Unavailable`. `PartyService::register`
  looks a **CNPJ** up and never a CPF:
  - found: stored as `Verified`, whatever the situation. A closed, suspended or inactive company is accepted and flagged
    (`needs_attention`), because inactive suppliers still appear on invoices and history;
  - `NotFound` (only a clear 404): rejected with `422`;
  - `Unavailable` (timeout, network error, 429, 5xx, a 400 that contradicts our own check digits, or lookup switched off):
    stored as `Unverified`. A third-party outage must never stop data entry.
- `infrastructure`: `BrasilApiCnpjRegistry` (5 s timeout) and an in-memory registry for tests and offline runs. Transport
  failures are logged with their cause by the adapter. The adapter sends a descriptive `User-Agent`: BrasilAPI answers `429`
  to requests without one, which only an end-to-end run against the real service revealed.
- `api`: `CNPJ_LOOKUP=brasilapi` (default) or `off`; anything else fails at startup. Responses carry
  `document_check { status, legal_name, situation, needs_attention }`.
- Only a CNPJ is sent to the third party: a public company identifier, never a CPF or a person's name.

## Consequences

- Tests never reach the real service: the adapter is tested against a local fake that mimics its statuses, and
  `11.222.333/0001-81`, the CNPJ used in examples, turns out to belong to a real company.
- `parties` gains `document_check`, `registry_legal_name` and `registry_situation` (CHECK constraints keep them consistent).
  Parties registered before this change with a CNPJ are marked `unverified`.
- There is no re-verification yet: an `Unverified` party stays that way until someone edits it. A "verify again" action is the
  natural next step.
- Rate limits are real (a handful of calls from one address were enough to trigger `429`). Installations that register many
  CNPJs at once should expect `Unverified` rows, run their own mirror of the open data, or add the Serpro adapter.
- Alphanumeric CNPJs are stored uppercase without punctuation. The web app no longer uses a numeric keypad for the document field.

## Revisit when

A free official endpoint appears, BrasilAPI changes its terms, or re-verification and bulk import (CSV) land and need caching
or batching.
