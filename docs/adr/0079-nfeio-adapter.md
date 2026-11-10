---
title: "ADR 0079: The NFE.io adapter"
status: accepted
date: 2026-11-10
tags: [adr, fiscal, phase-7]
---

# ADR 0079: The NFE.io adapter

## Context

[ADR 0077](0077-choosing-a-fiscal-gateway.md) compared Focus NFe and NFE.io and left the choice to the operator;
[ADR 0078](0078-gateway-reference-and-queued-events.md) gave the port what NFE.io needs (a reference the gateway assigns, events
answered later). This ADR is the adapter, `NfeIoGateway` (`FISCAL_GATEWAY=nfeio`). **The choice between the two gateways is still the
operator's** (prices are not public); building the second adapter only makes both available.

**How it was written.** From NFE.io's published OpenAPI file for the product invoices API v2
(`https://nfe.io/docs/api/nf-produto-v2.yaml`, read in full on 2026-11-10). **Nothing was exercised**: no account, no
certificate, no sandbox call. It is tested against a local stand-in of that API (`nfe_io.rs`), never the real one. An ignored test
(`the_sandbox_finds_what_it_made_so_a_second_submit_makes_none`, `NFEIO_SANDBOX_*`) is the homologation and only the operator can
run it.

## Decision

### Configuration

`NFEIO_API_KEY` (the invoice key, sent as `Authorization`), `NFEIO_COMPANY_ID_HOMOLOGATION` and/or `NFEIO_COMPANY_ID_PRODUCTION` (the
company's id at the gateway: the environment follows the company, not a flag on the request), `NFEIO_ISSUER_CNPJ` (one issuing CNPJ per
installation, as with Focus: a call for another company is `WrongIssuer` before anything is sent), optional `NFEIO_API_URL`
(`https://`, or exactly `http://localhost` / `http://127.0.0.1`, a stand-in) and `NFEIO_FILE_HOSTS`. The server refuses to start with one required value missing. The key is never stored or logged.
The certificate is uploaded at the gateway and never reaches this server.

### Never issue twice

The gateway names the invoice and documents no idempotency. So `submit_nfe` **searches before it submits**: it lists the company's
invoices with a query on the complementary information, where our reference is written (`Ref. cv-{our id}`, visible on the DANFE),
**checks the marker in the answer itself** (it does not trust the search to have filtered), ignores a document that ended in `Error`
(the person is sending it again) and takes the newest of several. **A search that fails, or that has more than one page of answers, is `Unavailable` and nothing is submitted.** An invoice that ended in `Error` or was `Disabled` does not count as the document. The gateway's id is returned as the document's reference
(`nfeio:{id}`), stored by the document (ADR 0078), and every later call uses it; asked about by our own id (the name was never
stored), the adapter finds it by the same search. An answer to the submit without an id is found by the search too, and if it is
not found the submission is `Unavailable` (the poll finds it later; it is never submitted again).
**The search's words were not verified.** Until the operator has run the sandbox test and sets `NFEIO_SEARCH_CONFIRMED=true`, **an
issue in production is refused** (`Unsupported`); homologation is allowed, because a duplicate there has no fiscal value.

### What is mapped

| Port | NFE.io |
|---|---|
| submit | `POST /v2/companies/{company}/productinvoices`: operation nature, `Outgoing`, `Normal`, the destination, the consumer type, the presence, the buyer, the items with `icms.origin` + `csosn`, `pis.cst`, `cofins.cst`, the payment, the moment in UTC; `400`/`422` is a rejection with the gateway's first error |
| query | `GET .../{id}`: `Created`/`Processing` waiting, `Issued` authorized (its XML and DANFE fetched), `IssuedContingency`, `IssueDenied` denied, `Error` rejected, `Cancelled`, `Disabled` rejected |
| cancel | `DELETE .../{id}?reason=` answers `204` "queued": `EventOutcome::Queued`; `400`/`404`/`422` a refusal |
| correction letter | `PUT .../{id}/correctionletter` `{reason}`: queued |
| ask about a cancellation | the invoice is `Cancelled` |
| ask about a letter | `GET .../{id}/events` has more letters than the authority had accepted before it |
| invalidation | `POST .../productinvoices/disablement` (series, UF, first, last, reason, environment): answers at once; `204` accepted, `400`/`422` refused, `409`/`503` unavailable |

Amounts are JSON numbers written from the exact decimal (the marker technique of ADR 0075, now shared in `json_numbers.rs`); the buyer's
tax number is a JSON integer, so **an alphanumeric CNPJ is refused** (it needs NFE.io's v3 API).

### The municipality's IBGE code

The NF-e needs the recipient's **IBGE municipality code** (`buyer.address.city.code`, required by the specification) and the registers keep only the
name and the state. The adapter asks **BrasilAPI's public list of the municipalities of the state** once per state and compares the name
without case or accents (`BrasilApiCityCodes`, behind the `CityCodes` trait). A name with no code, or two, is refused with the reason
(`Unsupported`), never guessed. **Verify** that the list covers the municipalities the company sells to; a code kept in the party register
would be better (a later slice).

### Files

The XML and the DANFE are served from an address the gateway answers with. It is followed **only over https and only to `nfe.io`, `nfse.io`
or a host the operator named (`NFEIO_FILE_HOSTS`), with no redirect**, and a size limit; a local address is followed only when the gateway's own address is local (a stand-in). If real files are served from a storage host, the
operator names it; until then an authorized document stays waiting (never lost) instead of fetching from an unnamed host.

## What is not verified, and the consequence

- **The search's words** (`additionalInformation.taxpayer:"..."`): the production guard above.
- **Where files are served from**: `NFEIO_FILE_HOSTS`.
- **How a refused cancellation or letter shows**: the specification gives no refusal to read, so a cancellation that is not done stays
  `sent` (the person gives up on it after checking at the gateway, ADR 0078) and a letter is accepted only when the events show one more
  than before (the event type is matched by the words `correction` or `cce`).
- **The cancellation's XML** is not fetched (the authority's events were not verified): a cancelled document is told cancelled **with no
  cancellation file**, so the accountant's package carries none for it.
- **The authority's protocol** is not in the answer the specification documents: an authorized document has none.
- **Whether the search index is as fresh as the write.** A retry right after a lost answer could miss a document the index does not show yet; only the production guard covers it, and the sandbox test should wait for it.
- **A CPF with a leading zero** goes as a JSON integer (the v2 contract) and loses the zero: check it in the sandbox.
- **That the stand-in is the real API.**

## Consequences

- Both gateways are available; an installation uses one (`FISCAL_GATEWAY`), and changing finishes or cancels what is waiting first.
- NFE.io's account holds several companies, but this adapter, like Focus, holds one issuing CNPJ per installation: lifting that is a later ADR.
- The mark on the DANFE (`Ref. cv-...`) is a cost of finding what we made without the gateway's id.

## Revisit when

The sandbox shows how the search, the events and the files really behave; the v3 API is needed (alphanumeric CNPJ); or the party register
keeps the IBGE code.
