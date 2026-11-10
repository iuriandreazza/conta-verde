---
title: "ADR 0077: Choosing a fiscal gateway: Focus NFe or NFE.io"
status: accepted
date: 2026-11-10
tags: [adr, fiscal, phase-7]
---

# ADR 0077: Choosing a fiscal gateway: Focus NFe or NFE.io

## Context

[ADR 0006](0006-fiscal-last.md) and [ADR 0073](0073-fiscal-module-plan.md) decided that the product issues fiscal documents
**through a gateway** behind the `FiscalGateway` port, and that the first real adapter is Focus NFe
([ADR 0075](0075-fiscal-gateway-port.md)). The question came up whether the product can **also** work with
[NFE.io](https://nfe.io/), and what it costs to issue. This ADR records what was found, what the choice depends on, and what
is and is not built. **The operator must choose one gateway per installation**; the product cannot choose for them, because the
decision rests on prices and coverage that are not public (see "What decides it").

**How this was read.** NFE.io publishes its OpenAPI files (`https://nfe.io/docs/api/nf-produto-v2.yaml`, `nf-consumidor-v2.yaml`).
The product-invoice file was read in full on 2026-11-10. **Nothing was exercised**: there is no account, no certificate and no
sandbox call. Prices of neither gateway are published on their sites' front pages, so none is quoted here.

## What every route costs, gateway or not

- The **A1 digital certificate** is mandatory to sign any fiscal document and is paid and renewed by the company.
- The company's **registration as an issuer** (state registration, municipal registration) is done with the tax authorities, with
  the company's accountant.
- Signing and sending the XML **ourselves** (no gateway) removes the gateway's fee and replaces it with building and maintaining
  XML signing, every state's web service, contingency and every layout change; it also means the certificate and its password
  live in our process, which ADR 0073 refused. It is not proposed here.

## The two gateways, as their public documentation describes them

| | Focus NFe (adapter built, [ADR 0075](0075-fiscal-gateway-port.md)) | NFE.io (read, not built) |
|---|---|---|
| Documents | NF-e, NFC-e, NFS-e (national standard), manifestation | NF-e, NFC-e, NFS-e (the NFS-e API is the older v1), distribution of received NF-e |
| Who names the document | **The caller**: a `ref` of ours, so asking twice is safe | **The gateway**: the `POST` queues the issue and the invoice has an id it assigns; the OpenAPI documents **no client reference and no idempotency** for issuing (the request body has an `id` field described only as "unique identifier") |
| Issuing | asynchronous | asynchronous (`200`: "queued") |
| Cancel, correction letter | synchronous answer | **asynchronous**: `204` "queued", the result comes by **webhook** (or by reading the invoice again) |
| Invalidation of numbers | `POST /v2/nfe/inutilizacao`, synchronous | a **range** endpoint `POST .../productinvoices/disablement` that is synchronous and idempotent, and a per-invoice one that is asynchronous |
| Authentication | HTTP Basic, one token per environment | `Authorization: <API key>`; a **data key** for lookups and an **invoice key** for issuing and management |
| Company | one token per company (one issuing CNPJ per installation, ADR 0073) | a `companyId` per CNPJ **in one account**, several CNPJs at no extra charge per their site; certificates are managed per company through the API (`/companies/{id}/certificates`) |
| Environment | chosen by the token | a property of the company's registration and a required `environment` query on lists (`Production` or `Test`) |
| Alphanumeric CNPJ | not checked | **not served by v2** (the notes are not exposed; a lookup answers `404`); needs the v3 API |
| Tax calculation | none (we send the codes the company registered) | the same, **but a paid "calculation engine" is triggered** when an item carries `taxDetermination`; sending the taxes in `tax` avoids it |
| Maturity | documented, widely used | pages disagree: one calls the NF-e and NFC-e APIs beta, another documents a v3 revised in October 2025; **confirm with them** |

> **Update:** the two changes below were made by [ADR 0078](0078-gateway-reference-and-queued-events.md) and the adapter was written
> by [ADR 0079](0079-nfeio-adapter.md) (`FISCAL_GATEWAY=nfeio`, **unverified against the real service**). The operator's choice
> between the two is still open and still rests on prices and coverage that are not public.

## What an NFE.io adapter needs that the port does not have

Writing the adapter against the port as it is today would be unsafe, because of two differences found in the real specification:

1. **A reference the gateway assigns.** The port's reference is **our document id**, and its contract is that asking again with
   it is safe ([ADR 0075](0075-fiscal-gateway-port.md)). NFE.io returns its own invoice id and documents no way to make a second
   `POST` return the first invoice. Without it, an answer lost in transit followed by a second submit **can issue two fiscal
   documents for one sale**. The port needs an opaque `gateway_reference` the adapter returns on the first answer and the
   document stores (a column, a CHECK, the audit trail), passed back on every later call; **and** a way to find an invoice we may
   have created without learning its id (NFE.io lists with an Elasticsearch `q`, whose searchable fields are not verified).
2. **Asynchronous events.** Cancel and the correction letter answer "queued", not "done". The port's `EventOutcome` has only
   `Accepted` and `Refused`; it needs a third answer, "queued", after which the event stays `sent` ([ADR 0076](0076-nfe-documents.md))
   and **a pass settles it** (reading the invoice's status and events, or a webhook route as slice 7 planned), instead of the person
   having to give up on it.

Smaller points: the company id and the two keys come from the environment (`NFEIO_COMPANY_ID`, `NFEIO_API_KEY`,
`NFEIO_DATA_KEY`); the environment follows the company's registration rather than a token; the request carries the amounts as
JSON numbers (the marker technique of ADR 0075 applies); and the buyer's CPF/CNPJ is a JSON **integer**, so a leading zero must be
restored when reading it back.

## What decides it

None of this can be settled from the code: **ask both for a quotation** for the expected number of documents per month and the
documents needed (NF-e only, or NFS-e and NFC-e too), and weigh:

- **Price per document and the monthly minimum** (not published on either front page).
- **Several CNPJs in one installation:** NFE.io's account holds several companies, which would lift the one-issuing-CNPJ limit of
  ADR 0073; Focus needs one token per company and the product holds one.
- **NFS-e coverage of the cities** the company issues in, and whether the national standard is enough.
- **Support and the stability of the API** (NFE.io's NF-e v2 is frozen for numeric CNPJ and v3 is the future).

## Decision

- **Focus NFe stays the one real adapter** and `FISCAL_GATEWAY=focus` the way to use it. Everything above the port (documents,
  events, the package, the screens) is independent of the gateway.
- **NFE.io is not built (as of this ADR; see the update above).** It becomes a second adapter **only after the port gains the two things above** (a gateway reference and
  an asynchronous event answer), which is its own change with its own ADR and a migration, and after the operator has chosen it:
  building it first on guesses risks double issuing.
- **An installation uses exactly one gateway**, named by `FISCAL_GATEWAY` (`off`, `fake`, `focus`; `nfeio` when it exists). The
  settings page tells which one is configured. A company that moves from one gateway to the other **finishes or cancels what is
  waiting first**: a document's reference means something only to the gateway that made it.
- The roadmap carries an explicit item, "choose the fiscal gateway", which a person ticks after comparing the quotations.

## Consequences

- Nobody is told that NFE.io works: it is documented as researched and not integrated.
- The choice between the two is a business decision the project cannot take, and it is written down so it is not forgotten.
- If NFE.io is chosen, the port change comes first; Focus is unaffected by it (it returns no reference and answers events at once).

## Revisit when

A quotation settles the choice; NFE.io confirms an idempotency mechanism for issuing; or an installation must issue for several
CNPJs.
