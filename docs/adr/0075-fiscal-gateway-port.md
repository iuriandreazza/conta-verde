---
title: "ADR 0075: The fiscal gateway port, its fake and the Focus NFe adapter"
status: accepted
date: 2026-11-09
tags: [adr, fiscal, phase-7]
---

# ADR 0075: The fiscal gateway port, its fake and the Focus NFe adapter

## Context

[ADR 0073](0073-fiscal-module-plan.md) decided that fiscal documents are authorized through a gateway behind a port, with a
fake for every test and Focus NFe as the first real adapter, that the gateway token lives in the environment, and that the
gateway keeps the certificate and assigns the numbers. This slice is the port and its adapters, before any document is made
from a sale (slice 4): what the application may ask a gateway, what a gateway may answer, how the answers are told apart, and
how the real adapter is tested without a real gateway, certificate or CNPJ.

## Decision

### The port

`FiscalGateway` (`application`, `fiscal_gateway.rs`) has five calls for the NF-e, each returning
`impl Future<Output = Result<_, GatewayError>> + Send`, each about the issuer in the `FiscalSettings` it is given (the
snapshot of [ADR 0074](0074-fiscal-settings.md)): `submit_nfe(&NfeRequest)`, `query_nfe(issuer, reference)`,
`cancel_nfe(issuer, reference, reason)`, `correct_nfe(issuer, reference, text)` and
`invalidate_nfe_numbers(issuer, series, first, last, reason)`. NFS-e and NFC-e add their calls with their own slices: the port
grows by slice, and an adapter that cannot do a new call yet answers `Unsupported`.

- **What an answer about a document is** (`DocumentOutcome`): `Processing` (sent, not answered: ask again), `Authorized`
  (the access key, the number and series the gateway assigned, the protocol, the XML and, when there is one, the DANFE PDF,
  **as bytes**: the adapter downloads them, the use case stores them in the `BlobStore`), `Contingency` (issued while the
  authority was out, still waiting: ask again), `Rejected` and `Denied` (a `Refusal`), `Cancelled` (the authorized document
  and the cancellation's XML) and `Unknown` (the gateway has no document with this reference, which is how a use case learns
  that a submission never got there).
- **What an answer about an event is** (`EventOutcome`): `Accepted` (the protocol, the correction letter's sequential number,
  the XML and the PDF when there are any) or `Refused` (a `Refusal`). A refusal by the authority is an **answer**, not an
  error: the cancellation window, a letter that changes values, a range already used.
- **A `Refusal`** is the authority's or the gateway's code and text as the person must read them to fix the document. They are
  stripped of control characters and cut (50 and 500 characters) so a row can hold them. This is the one place the product
  shows a third party's text, and it carries no credential ([ADR 0073](0073-fiscal-module-plan.md)).
- **A `GatewayError`** is a failure to get an answer, and its text is for the log and never for a client:
  `Disabled` (no gateway configured), `WrongIssuer` (the gateway is configured for another CNPJ), `Unsupported` (this
  adapter does not do it: an NF-e for Lucro Presumido, which waits for the tax calculation of slice 8, or a reference that is
  not a plain id) and `Unavailable` (network, timeout, the token refused, an error of the gateway, an answer it does not
  understand: asking again later is the remedy).
- **Idempotency is the reference, for the document only.** Our id of the document is the gateway's reference: `submit_nfe` with the same reference
  again gives the state of the document it made, never a second one, and the numbers are the gateway's. A refused document may
  be sent again under the same reference (**not verified** that the real gateway allows it: the fake does, and the adapter reads a `422 already_processed` as "query"). The **events** are not idempotent: after an `Unavailable` the caller cannot know whether the authority took a cancellation, a correction letter or an invalidation, so slice 4 asks `query_nfe` first (a cancelled document says so) and records what it sent before sending, never repeating one blindly.

The request (`NfeRequest`) carries the issuer's settings, the nature's name, the moment in Unix seconds (the caller's clock:
the adapter writes it with the clock of the issuer's state, `domain::fiscal_timestamp`; offsets -03:00, -04:00 for AM, RO, RR,
MT and MS, -05:00 for AC, no daylight saving: verify with an accountant if it returns), the destination, whether the
buyer is a final consumer, how present the buyer was, the recipient, the items with their classification and the
payments and totals. It carries **no tax values**: for the Simples Nacional the classification (CFOP, CSOSN, PIS and COFINS
codes) is all the layout asks, and the arithmetic for the regular regimes is slice 8.

### The adapters

- **`NoFiscalGateway`** (`FISCAL_GATEWAY` unset or `off`): refuses every call with `Disabled`.
- **`FakeFiscalGateway`** (`FISCAL_GATEWAY=fake`, and every test): answers from memory, follows the real gateway's moves
  (an asynchronous authorization after a configurable number of questions, numbers it assigns per issuer, environment and
  series, a reference that makes asking twice safe, a refused submission that takes no number, correction letters numbered
  up to 20, a cancellation only for an authorized document, reasons of the lengths the gateway documents) and can be told to
  reject, deny, refuse events or be unavailable. Its access keys pass the domain's check digit
  (`AccessKey::from_parts`) and what it makes says in the XML and the PDF that it has **no fiscal value**. With it the page
  says so too.
- **`FocusNfeGateway`** (`FISCAL_GATEWAY=focus`): see below.

A shared **contract** (`contract::a_gateway_authorizes_cancels_and_corrects`) states what every adapter does (an unknown
reference is `Unknown`; a document is authorized at once or after asking again; the same reference is the same document; the
next number is the next; a correction letter needs its 15 characters and is numbered from 1; a cancellation needs its reason
and an authorized document, after it the document is `Cancelled`; a range is invalidated with a reason) and runs against the
fake and against the Focus adapter over a local stand-in.

### The Focus NFe adapter

Read from the gateway's public documentation (`doc.focusnfe.com.br`, November 2026); **none of it was exercised against the
real service**.

| Call | Request | Answers it understands |
|------|---------|------------------------|
| `submit_nfe` | `POST /v2/nfe?ref={reference}` with the document as JSON | `202` `processando_autorizacao` is `Processing`; `201` `autorizado` is `Authorized`; `422` `already_processed` and `pending_operation` mean "query" (never an error); any other `4xx` is a `Rejected` with the gateway's `codigo` and `mensagem` and the `erros[].campo` it lists; `401`, `403`, `408`, `429` and `5xx` are `Unavailable` |
| `query_nfe` | `GET /v2/nfe/{reference}` | `404` is `Unknown`; the `status` is `processando_autorizacao`, `autorizado`, `erro_autorizacao` (`Rejected` with `status_sefaz` and `mensagem_sefaz`), `cancelado`, `denegado`/`denegada` (`Denied`; the documentation does not list it); any other status is `Unavailable`, never a guess |
| `cancel_nfe` | `DELETE /v2/nfe/{reference}` with `justificativa` (15 to 255 characters) | `cancelado` is `Accepted` with the cancellation's XML; `erro_cancelamento` and any `4xx` are `Refused` |
| `correct_nfe` | `POST /v2/nfe/{reference}/carta_correcao` with `correcao` (15 to 1000 characters; at most 20 per document) | `autorizado` is `Accepted` with `numero_carta_correcao`, the XML and the PDF; `erro_autorizacao` and `4xx` are `Refused` |
| `invalidate_nfe_numbers` | `POST /v2/nfe/inutilizacao` with `cnpj`, `serie`, `numero_inicial`, `numero_final`, `justificativa` | `autorizado` is `Accepted`; `erro_autorizacao` is `Refused` |

Authentication is HTTP Basic with the token as the user and an empty password. The environment decides the host
(`homologacao.focusnfe.com.br`, `api.focusnfe.com.br`) **and the token**, since each environment has its own.

- **Amounts are never floats.** The gateway takes JSON numbers, so the body is built with a marker unique to the request
  where each exact decimal goes and the marker is replaced by the decimal text when the body is written (the technique of
  the Asaas adapter, [ADR 0049](0049-asaas-provider.md), for many numbers). A description that contains the marker's
  look-alike text cannot pass for a number, and a test sends one.
- **The files** are downloaded from the **path** the answer gives, joined to the gateway's own host and with the same
  authentication; a path that is not on that host is refused, so an answer cannot send the token elsewhere. A file over 10 MiB
  is refused.
- **Nothing sensitive reaches a log or an error.** The adapter holds no logger: a failure is a `GatewayError` whose text is a status or a fixed sentence (never a body, an address, a reference or a token) and the use case logs it. The client follows **no redirect**, a file path with `..` is refused, a file over 10 MiB is refused from its declared length and again from what was read, a success whose answer is not JSON is `Unavailable` (never read as a refusal), a `4xx` other than `400` and `422` on a submission is a problem of the call and not a refusal of the document, and a query the gateway refuses says nothing of the document. The key in an authorized answer must be this issuer's and agree with its number and series.
- **Configuration is the environment's** ([ADR 0073](0073-fiscal-module-plan.md)): `FISCAL_GATEWAY=focus`,
  `FOCUS_NFE_TOKEN_HOMOLOGATION` and/or `FOCUS_NFE_TOKEN_PRODUCTION` (at least one: a call in an environment without its
  token is `Unavailable` and says which), `FOCUS_NFE_ISSUER_CNPJ` (a valid CNPJ, required: a call for any other company is
  `WrongIssuer` before anything is sent) and an optional `FOCUS_NFE_API_URL` for a stand-in (an `https://` address or a local one: the tokens go there). The server does not start with
  the gateway named and a setting missing or wrong.
- **Not verified**, and what the ignored sandbox test exists to settle: the names of the request's fields beyond the sale
  the documentation shows (the tributable unit fields, the buyer's registration indicator, the payment group, the recipient's
  address), whether `chave_nfe` always carries the `NFe` prefix the adapter strips, how a document issued in contingency is
  reported (any status the adapter does not know is `Unavailable`, so it is asked about again, not misread), where the files
  are served from and whether they need the token, and whether `denegada` exists as a status.

### The route and the page

`GET /companies/{id}/fiscal/gateway` (the `fiscal` area, a read) answers `{"gateway": "focus" | "fake" | null}`, and the
settings page says when none is configured or when it is the fake. Nothing else calls the gateway yet: slice 4 makes the
documents.

### Tests

The domain tests the clock of each state, the keys built from their parts and the new refusals; the contract runs on the fake
and on the Focus adapter over a **local stand-in** of the documented API (it checks the token of the environment, assigns
numbers per series, answers `202` and then the document, and refuses what the gateway documents); the adapter's own tests cover
the token per environment, the wrong issuer, the exact decimals, a refusal read as the person must read it, `already_processed`
and `pending_operation`, a gateway that fails, a status it does not know and an address that is not in the log. **`the_sandbox_authorizes_an_nfe`** is ignored
and does nothing without `FOCUS_NFE_SANDBOX_TOKEN` and `FOCUS_NFE_SANDBOX_CNPJ`; run by hand it issues one NF-e in the gateway's
test environment for the company registered there. **The gateway requires a real CNPJ and a valid A1 certificate even in
homologation, so this test can only be run by the owner of one** ([ADR 0073](0073-fiscal-module-plan.md)).

## Consequences

- Slice 4 can write the document's state machine against a port that is already tested on both a fake and the real adapter's
  shape, with no network.
- An installation issues for one CNPJ; another company's documents are refused, not sent.
- A gateway that answers in a way this adapter does not know is asked about again, not guessed at.
- What the real gateway accepts is **unproven** until someone with a certificate runs the sandbox test.

## Revisit when

The sandbox test shows a field or a status this reading got wrong; an installation must issue for several CNPJs (tokens
encrypted at rest, [ADR 0073](0073-fiscal-module-plan.md)); another gateway should be an adapter; or the gateway documents a
signature for its notices, which slice 7 needs.
