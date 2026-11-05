---
title: "ADR 0024: Postal code (CEP) lookup behind a port"
status: accepted
date: 2026-11-05
tags: [adr, registers, integrations]
---

# ADR 0024: Postal code (CEP) lookup behind a port

## Context

[ADR 0022](0022-party-roles-address-and-editing.md) gave a party an address and checked only its
shape, leaving the CEP lookup for its own step: typing a street, a district, a city and a state is
the slowest part of registering a customer, and the CEP already says all four. The only complete
source is the Correios' own directory; the official web service needs a contract, and the free
public page sits behind a captcha. Free services that query the Correios and other public sources
exist, run by third parties.

Nothing here is personal data in the way a CPF is: a CEP names a street, not a person. It is still
sent to a third party that sees the installation's address, and it can be down or slow, so the
lookup follows [ADR 0017](0017-cnpj-registry-lookup.md): a port, an operator switch, and a failure
that never stops data entry.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Type the address by hand (today) | No network, no third party | The slowest part of registering a party |
| Correios web service (official) | Source of truth | Needs a contract and credentials per installation |
| Ship the whole CEP database | No network at runtime | Hundreds of thousands of rows to license, update and store |
| Free community API (BrasilAPI) behind a port (chosen) | No signup, one call, answers street, district, city and state | No uptime guarantee, shared rate limit, third party sees the CEP |

ViaCEP answers the same question and is a drop-in second adapter if BrasilAPI becomes a problem;
BrasilAPI is chosen first because the CNPJ lookup already depends on it, so there is one third
party to know about, one `User-Agent` and one set of limits.

## Decision

- `domain`: `PostalCode` is the CEP as a value: eight digits, written with or without the hyphen
  and kept without it. `Address` uses it, so the shape rule lives in one place. The port takes a
  `PostalCode`, never a string, so what leaves for the third party is always eight digits.
- `application`: the `PostalCodeDirectory` port answers a `PostalCodeEntry` (street and district
  when there are any, city and state) or `NotFound`, `Unavailable` or `Disabled`. `PostalCodeService`
  refuses a code that is not eight digits **without asking the directory**.
- `infrastructure`: `BrasilApiPostalCodeDirectory` calls `/api/cep/v2/{code}` (5 s timeout, the
  `User-Agent` BrasilAPI insists on, the HTTP client shared with the CNPJ adapter). The text comes
  from a third party and goes into a form, so it is bounded to the lengths an address accepts and
  stripped of control and direction characters. A city with a single code for all of it has an
  empty street, which is "no street", not an error; an answer without a city or a two-letter state
  counts as unavailable. `InMemoryPostalCodeDirectory` is a test double; tests never reach the
  real service.
- **`404` is not proof.** BrasilAPI answers `404` both for a code it does not know and when every
  one of its own sources failed (checked against the real service: a code that exists answers
  `404` while its backends are down). The adapter maps `404` to `NotFound`, and the message says
  "no address found, or the service could not tell: check the number or fill the address in by
  hand". Unlike a CNPJ, nothing is stored about the answer, so there is nothing to mislabel.
- `api`: `GET /companies/{company_id}/postal-codes/{code}` answers `200` with `street`, `district`,
  `city` and `state`; `422` for a code that is not eight digits, `404` for no address, `503` when
  the service did not give a usable answer, `409` when the operator switched it off. It needs a
  session and membership like every company route, so the installation is not an open proxy to
  BrasilAPI. `CEP_LOOKUP=brasilapi` (default) or `off`; anything else fails at startup, like
  `CNPJ_LOOKUP`.
- **Nothing is stored or recorded.** A lookup is not a change to any record, so there is no audit
  entry and no table. The answer only fills a form; whoever saves it goes through the same checks
  as an address typed by hand (ADR 0022), including the state being one of the 27.
- **Web:** a "Buscar endereço" button beside the CEP, enabled at eight digits. It is a button, not
  a lookup on every complete CEP, so nothing is sent to a third party until the person asks. It
  replaces the street, district, city and state with what the service knows (the person asked for
  them), never the number or the complement, and moves the cursor to the number. A code without a
  street fills the city and state and says so. A failure is a warning and the form works as before.

## Consequences

- Operators that must not send a postal code to a third party, or run without internet, set
  `CEP_LOOKUP=off`; the button then answers that the lookup is off, and the address is typed.
- There is no cache and no circuit breaker: a lookup is one call that a person asks for, once per
  address, unlike a CNPJ that can be re-checked in bulk. The rate limit is shared by the whole
  installation (and BrasilAPI's own sources have their own), and there is no per-user throttle:
  any signed-in user can use up the quota by looping over codes. A rate limiter in front of the API
  is the real remedy, as in ADR 0017.
- The CSV import (next) does **not** look addresses up: a row either has its address or not.

## Revisit when

BrasilAPI changes its terms or its `404` stops covering "all sources failed" (a cache of found
codes then becomes worth having), a bulk operation needs addresses filled in, or a second adapter
(ViaCEP) is wanted as a fallback.
