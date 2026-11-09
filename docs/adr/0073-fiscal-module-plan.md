---
title: "ADR 0073: The fiscal module: plan, shared rules and what it changes underneath"
status: accepted
date: 2026-11-09
tags: [adr, fiscal, phase-7]
---

# ADR 0073: The fiscal module: plan, shared rules and what it changes underneath

## Context

Phase 7 makes the product issue fiscal documents: NF-e for products, NFS-e for services, NFC-e at the counter, and the
recipient's manifestation for the NF-e other people issue to the company. [ADR 0006](0006-fiscal-last.md) (accepted) decided
the shape: fiscal comes last, behind a port, through a paid emission gateway, with tax calculation as pure functions that an
accountant reviews. Nothing else in the phase's list is `proposed`: [ADR 0015](0015-public-api-and-webhooks.md) belongs to
Phase 8. This ADR settles, once, what the slices share and what the phase changes underneath earlier work; each slice's ADR
says only what is new.

**What this ADR could not check, and says so.** The author is not an accountant and has no gateway account or certificate.
Everything about Brazilian tax law below is marked **verify with an accountant** where it is a rule and not a design choice,
and everything about the gateway's API was read from its public documentation (`doc.focusnfe.com.br`, November 2026), not
exercised. Where the documentation was silent the text says "not verified". The product embeds **no tax rates and no legal
tables** (see "What the product knows of tax law") precisely so that the part nobody here can vouch for stays in data the
company and its accountant own.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Sign and send the XML ourselves to each state's SEFAZ | No third-party cost | Rejected in ADR 0006: XMLDSig, schemas, contingency and every state and municipality as a maintenance tail |
| A gateway behind a port (chosen) | One HTTP integration; the gateway tracks protocol changes | A paid dependency for most users; a fake and one real adapter needed |
| Hold the A1 certificate ourselves, encrypted at rest | Self-contained | A company's private key in our database is the largest liability of the product; a new crypto crate and key management |
| The certificate stays at the gateway (chosen for v1) | The product never holds a key or its password | The person uploads it in the gateway's panel; the product can only be told its validity |

**The provider.** The first real adapter is **Focus NFe**: its API is public and documented, it has a homologation
environment at a separate host, it covers NF-e (issue, query, cancel, correction letter, invalidation), NFC-e, NFS-e
through the national standard (DPS), and the manifestation of received NF-e, and it identifies each document by a
reference chosen by the caller, which is the idempotency this product needs. Other gateways that fit the same port are
welcome as adapters; none is promised. The adapter is `FocusNfeGateway`, chosen by `FISCAL_GATEWAY=focus`.

## Decision

### The slices

One pull request each, with its own ADR where it has something new to decide, a migration where it needs one, both adapters,
the routes and the pages. The order of the roadmap holds and nothing is reordered. The settings (item 1) come before the
port (item 2) only because the certificate is **not** uploaded through the product (below): had it been, the settings could
not show its validity before the gateway existed, and the order would have had to change.

1. **This plan.**
2. **Fiscal settings** (regime, issuer identity, series, the certificate's metadata, operation natures, tax rules).
3. **The port, the fake and the Focus adapter.**
4. **NF-e**: issue from a sale, DANFE, cancel, correction letter, invalidation, contingency, the screens, and the XMLs in
   the accountant's package.
5. **NFS-e** through the national standard.
6. **NFC-e** and a minimal point of sale.
7. **Manifestation** of received NF-e (and the one public route this phase needs).
8. **The tax calculation and the accountant's checklist.** The calculation is **built and tested** (its worked examples are
   the tests) but **its review cannot be done by a program**: the roadmap item stays unticked until a human accountant has
   signed the checklist.

The tax calculation (8) is needed before NF-e can carry values in Lucro Presumido, so slices 4 to 6 ship **Simples
Nacional only** and refuse the other regime with a message (`422`); slice 8 opens it. Simples Nacional on the documented
simple cases needs a classification (CFOP, CSOSN, a PIS/COFINS code), not arithmetic.

### The `fiscal` area and who may use it

A new area `Fiscal` (`fiscal` in the permissions the web app reads) holds the route segment `fiscal`: settings, documents,
events, invalidations, the manifestation. It is a new row of the table of [ADR 0026](0026-membership-roles-and-permissions.md)
and [ADR 0067](0067-accountant-and-reporting-plan.md), so `Area::ALL` grows to eight, `SEGMENT_AREAS` gains the segment,
and the router-walk test of ADR 0067 covers it:

| role       | parties | products | finance | sales | purchases | accounting | fiscal | members |
|------------|---------|----------|---------|-------|-----------|------------|--------|---------|
| owner      | write   | write    | write   | write | write     | write      | write  | write   |
| admin      | write   | write    | write   | write | write     | write      | write  | write   |
| financial  | write   | read     | write   | read  | read      | read       | write  | -       |
| sales      | write   | read     | read    | write | read      | read       | read   | -       |
| stock      | read    | write    | read    | read  | write     | read       | read   | -       |
| accountant | read    | read     | read    | read  | read      | write      | read   | -       |
| viewer     | read    | read     | read    | read  | read      | read       | read   | -       |

Why: issuing an invoice is a legal act of the company, so the person who handles its money writes it and the people who
sell only read and download what was issued (a salesperson who must issue gets the `financial` or `admin` role; **revisit**
if that proves too strict). The accountant reads everything fiscal and downloads the XMLs through `accounting`, and writes
nothing here: ADR 0067 keeps the accountant's only write area as `accounting`. The settings (regime, issuer, series) are in the
same area, so a `financial` member can change them; every change is audited with the old and new value.

### The port: `FiscalGateway`

ADR 0006 called it `FiscalEmitter`. It is **`FiscalGateway`**: it also queries, corrects, invalidates and manifests, and the
name says what the adapter is (a gateway), not one of its verbs. It lives in `application` like `PaymentProvider`
([ADR 0010](0010-payment-providers.md)): methods return `impl Future + Send`, an adapter has a `key()` and `is_enabled()`, and
the off adapter (`FISCAL_GATEWAY` unset) refuses every call with `ProviderError::Disabled` so the settings and the documents
still work and issuing says why it cannot. Its calls, each taking an issuer context (company, CNPJ, environment) and our
document id as the gateway's **reference**:

- `submit` a document (NF-e, NFS-e, NFC-e) -> `Authorized {key, number, series, protocol, XML, PDF}`, `Processing`,
  `Rejected {code, message}`, `Denied`, `Contingency`;
- `query` the same outcome by reference;
- `cancel`, `correct` (correction letter), `invalidate` (a range of numbers), `manifest` (for received NF-e).

The adapter downloads the XML and the printed form itself and returns **bytes**; the use case stores them. The gateway's
refusals travel as `Rejected` with the SEFAZ's code and message, which are shown to the person (unlike the payment
providers, whose text never reaches a client): a rejection's text is what the person must read to fix the invoice, it carries
no credential, and it is stored on the document cut to 500 characters without control characters. The adapter's own failures
(network, `5xx`, unknown answers) are `Unavailable` and logged by status only, never with a body: a body carries the
customer's data and, on one call, a certificate.

A `FakeFiscalGateway` is the adapter of every test: deterministic, with **access keys that pass the domain's check digit**
(ADR 0057), the same state transitions as the real one, configurable to reject, to stay `Processing` for N queries and to be
unavailable. **Tests never reach the real gateway.**

### The provider's shape, as read from its documentation (verify at homologation)

Base URLs `https://api.focusnfe.com.br` (production) and `https://homologacao.focusnfe.com.br`, routes under `/v2`, HTTP
Basic with the token as the user and an empty password. The environment is chosen by the **token**: the empresa's response
carries `token_producao` and `token_homologacao`. Issue is `POST /v2/nfe?ref=` and is **asynchronous by default** (`202` with
`processando_autorizacao`; `201` when the state allows it to be synchronous); query is `GET /v2/nfe/{ref}`; cancel is
`DELETE /v2/nfe/{ref}` with a `justificativa` of 15 to 255 characters, synchronous; the correction letter is
`POST /v2/nfe/{ref}/carta_correcao` with a `correcao` of 15 to 1000 characters, synchronous, at most 20 per document;
invalidation is `POST /v2/nfe/inutilizacao` (`cnpj`, `serie`, `numero_inicial`, `numero_final`, `justificativa`); manifestation is
`POST /v2/nfes_recebidas/{key}/manifesto` with `tipo` (`ciencia`, `confirmacao`, `desconhecimento`, `nao_realizada`); NFC-e is
synchronous and NFS-e national is issued from a DPS. Money in the request is a JSON **number**: the adapter writes it from the exact
decimal text with the placeholder technique of the Asaas adapter ([ADR 0049](0049-asaas-provider.md)), never through a float.
**Not verified:** the exact field names of every request beyond the simple sale, the host and authentication of the file
paths the answers return (`/arquivos/...`), how a document issued in contingency is reported, whether `denegada` exists as a
status, and whether the master token can create a company for another token. The ignored sandbox test of slice 3 exists to
settle them.

### Where credentials and the certificate live

- **The gateway token is read from the environment**, never stored in the database and never logged: `FISCAL_GATEWAY=focus`,
  `FOCUS_NFE_TOKEN_HOMOLOGATION`, `FOCUS_NFE_TOKEN_PRODUCTION`, `FOCUS_NFE_ISSUER_CNPJ`, optional `FOCUS_NFE_API_URL` and
  `FOCUS_NFE_WEBHOOK_TOKEN` (at least 32 characters; slice 7). The server refuses to start with the gateway named and a token
  missing, as it does for the payment providers.
- **Cost of that choice, written down: one issuing CNPJ per installation.** A token belongs to one company at the gateway, and
  holding several in the database would need them **encrypted at rest** with a key from the environment, which means a new
  crypto crate and key management. That is a later ADR if an installation must issue for several CNPJs. Meanwhile the adapter
  refuses, before calling anything, a company whose CNPJ is not `FOCUS_NFE_ISSUER_CNPJ`: a document must never go out under
  another company's token.
- **The A1 certificate (PFX) and its password never enter the product in v1.** The person uploads it in the gateway's panel,
  the documented route (the company's registration there also takes it). What the product keeps, as the company's own **settings**,
  is the certificate's *metadata*: the day it expires, which the person types from the certificate (and which the port may later
  confirm), a warning in the app 30 days before and **issuing refused after it** (`422`, a pure comparison of the settings' date
  with the moment the caller passes in). The gateway documents `POST /v2/empresas` with `arquivo_certificado_base64` and
  `senha_certificado`, and an answer with `certificado_valido_ate` and `certificado_cnpj`; sending it through the product would
  put the key and its password in our process, so it waits until it can be verified (the master-token question above) and is
  its own ADR. The roadmap item "A1 certificate handling" is therefore ticked as **metadata and expiry**, and the report says
  that upload is not built.
- **Nothing sensitive is logged**: the adapter logs status and the gateway's own error code, never a request or a response body.

### What the product knows of tax law

**Nothing built in.** There are no ICMS, PIS, COFINS or ISS rates, no CFOP or CST table, and no list of which codes go
with which regime in the code. The company (with its accountant) registers its **operation natures** (a name, a kind
`product`/`service`, the CFOP for a sale inside the state and for one to another state, the finality, whether it moves
stock) and **tax rules** (the codes ICMS/CSOSN, PIS and COFINS and, for the regimes that need them, the rates, for a nature,
optionally narrowed by a product's NCM prefix). The domain checks **shape** (four digits and a leading digit for a CFOP,
three digits for a CSOSN or CST, rates with at most two decimals between 0 and 100) and does the **arithmetic** (the one
rounding rule of sales, `round_to_cents`, half away from zero, [ADR 0037](0037-sales-module-plan.md)). **Verify with an
accountant:** that the codes of the examples in the tests suit a real company, every CFOP and CSOSN default the settings page
may suggest, the rounding rule for tax values, and what changes for MEI, Simples with excess of sublimit and Lucro Real, which
are **refused** (`422`) in v1. Slice 8 lists every assumption in this ADR's checklist for the accountant.

### The document and its state machine

A fiscal document (`FiscalDocument`, `fiscal_documents`) has a **kind** (`nfe`, `nfse`, `nfce`), the **issuer's snapshot**
(CNPJ, name, regime, environment: a document is never reinterpreted by a later change to the settings), the **recipient's
snapshot**, an **origin** (a sale, a return later; nothing for a point-of-sale sale that makes no sale record is decided in
slice 6), the lines with their classification and values as sent, and what the gateway answered. Our **id is the gateway's
reference**: asking the gateway twice with it is safe, which is the document's idempotency. States:

```
draft -> processing -> authorized -> cancelled
                  |        ^  \-> (events: correction letters, never a state)
                  |        |
                  +-> contingency -+
                  +-> rejected  -> processing   (sent again, same reference)
                  +-> denied
```

- **draft**: the record exists, nothing was sent. **processing**: sent, the gateway answered `202`/`processando_autorizacao`.
- **authorized**: the SEFAZ authorized it; the access key, number, series, protocol and the files are stored. From here the
  **content never changes**: only the status and the appended events do (enforced by the use cases and by the adapters, and by
  a trigger in the migration that refuses an update of the content columns once authorized, decided in slice 4).
- **rejected**: refused; it keeps the code and the text, and may be sent again (the person fixes what the sale or the
  registers lacked) under the same reference.
- **contingency**: issued while the SEFAZ was out and still **waiting for authorization**. The product **never chooses a
  contingency mode** (a legal choice); it shows what the gateway reports and goes on asking until authorized or rejected.
- **denied** (denegada): terminal, the SEFAZ refused for a reason about the issuer or the recipient. **cancelled**: terminal.
- **Invalidation** (inutilização) is **not a state of a document**: it declares a **range of numbers** of a series unused, so
  it is its own record (`FiscalInvalidation`: series, from, to, the reason, the outcome), never edited.
- A correction letter (CC-e) is an **event** appended to an authorized document (number 1 to 20, its text, the protocol, its
  XML), never a state.
- **At most one live document per origin and kind** (`draft`, `processing`, `authorized`, `contingency`): a partial unique
  index, so asking to issue twice finds the same document. A `rejected`, `cancelled` or `denied` one frees the sale.
- **Every move is audited** ([ADR 0008](0008-audit-trail.md)) in the transaction that writes it, in the name of whoever asked.
  What happens later without a person (the poll below) is written **in the name of whoever submitted the document**, which the
  document records, as charges do ([ADR 0045](0045-charges.md)).
- The gateway's `pending_operation` and `already_processed` answers mean "ask for the state", **never an error**.

**Issuing is stepwise and every step can be repeated**, like completing a sale ([ADR 0039](0039-sales.md)): (1) validate the
sale and the settings and write the `draft` (the unique index is the claim); (2) mark it `processing` and `submit`; (3) store the
outcome and, if authorized, the files. A failure after any step is finished by asking again. `processing` documents are
finished by a **`FiscalPoll` job** on the queue ([ADR 0059](0059-job-queue.md); a new kind with a value in the table's `CHECK`),
which queries and applies the answer, harmless to repeat; a document that is still `processing` after the queue gave up is
refreshed by the person (`POST .../refresh`). **No server-side clock for deadlines**: the 24 hours to cancel an NF-e, the 720
hours and the 20 letters for a CC-e, the 30 minutes of an NFC-e and the states that allow longer are the **SEFAZ's and the
gateway's** to enforce; the product sends the request and shows the refusal (`422` with the text), and never computes them
(**verify with an accountant** that no deadline should also be shown in advance). The moment of a document is passed in by the
caller (as for document mails, [ADR 0065](0065-document-mail-limits-and-log.md)) so tests move it.

### Numbering and series: the gateway owns the number

A document's number is assigned by **the gateway** (it keeps "next number" per company, series and environment, and the SEFAZ
sequence must have no duplicates); the product does **not** keep its own counter for fiscal numbers
(`document_counters` of [ADR 0037](0037-sales-module-plan.md) is for quotes and sales). The document **records the number and
series the gateway returned**. The settings hold the series the company uses for each kind (default 1) because an
invalidation needs one and because it is what the person expects to see; a document that comes back with another series is
shown as it is. **Gaps** come from numbers consumed by an attempt that never became an authorized document; the product does
not detect them in v1: the person asks for an invalidation of a range, with a reason of at least 15 characters, which the
gateway sends. (Not sure exactly when the gateway burns a number; **verify with the sandbox and an accountant**.) The fake
follows the same rule: it assigns increasing numbers per series.

### Files: the BlobStore and the accountant's package

- **The XML and the DANFE PDF** (and the XML of each event) are stored in the `BlobStore` ([ADR 0036](0036-entry-attachments.md)),
  never in the database. The store is keyed today by `AttachmentId`; slice 4 **generalizes the key to an opaque `BlobId`** (a
  UUID, `AttachmentId` converts into it) rather than adding a second store, and a `fiscal_files` record (company, document,
  kind, blob id, size, SHA-256) says whose a file is. Files are written before the record that names them and removed with
  nothing: a fiscal file is **never deleted**.
- **This ADR amends [ADR 0071](0071-accountant-exports.md)**, which fixed the package's contents and said fiscal XMLs wait for
  this phase. From slice 4 the package also holds, for the documents **issued in the span**, `fiscal.csv` (one row per
  document: kind, number, series, key, status, issue day, recipient, total, environment; columns fixed in slice 4's ADR) and
  the files under **`fiscal/nfe/{access_key}.xml`**, `.pdf`, and for events `{access_key}-cancel.xml` and
  `{access_key}-cce-{n}.xml` (`fiscal/nfce/...`, `fiscal/nfse/...` likewise; a document with no access key is named by its
  id). The files **count toward the existing limits** of 500 files and 200 MiB, checked before any file is read, answering
  `422` rather than cutting. Only documents of the production environment are in the package unless the person asks for the
  homologation ones (a test document is not a fiscal document); a rejected or denied document has no files. The "Fiscal" line
  of the Contador page stops saying that the product keeps none.

### The period lock

A fiscal document is in the books by its **issue day** (the Brazil day of the moment it was issued). Issuing is allowed in
any open day (it is issued *now*, never into a closed month: the moment is the caller's, so a lock never refuses a today).
**Cancelling or invalidating** a document whose issue day is on or before the lock date is refused, **`409 PeriodLocked`**:
the accountant closed that month with that document in it, and withdrawing it changes the taxes of a closed month (to cancel
it the month is reopened, with a reason, as ADR 0069 requires for any change to a closed day). A **correction letter** changes
no figure (the SEFAZ refuses a letter that changes values) and is allowed, like an attachment. A document that authorizes late
(`processing` or `contingency` finishing) is written whatever the lock says, with a note, as a payment webhook is
([ADR 0069](0069-period-lock.md)). Cancelling the **sale** and cancelling its **invoice** are separate acts: neither does the
other, and the sale's page says so.

### Public routes and web

The only public route this phase adds is the gateway's webhook (slice 7, for received NF-e and faster answers):
`POST /public/fiscal/{company_id}/webhooks/focus`, with **no session**. The gateway lets us choose an `authorization` header
value and sends it with each notification, and documents **no signature**, so the route is a **wake-up only**: it compares that
header in constant time with `FOCUS_NFE_WEBHOOK_TOKEN` (`401` otherwise), **trusts nothing of the body except the reference it
names**, and re-reads the state with an authenticated `GET` to the gateway. A forged or replayed notice can only make us ask.
Until it exists the poll job is the only way a `processing` document finishes.

Each slice ships its pages in `web/` with Vitest tests that assert what the person sees, checked in the real app: gating on
`useCanWrite("fiscal")`, `PageHeader`, `usePagedList`, the global sheet pattern, the two-level menu (a **Fiscal** group:
Documentos, Configurações), the explanations behind an (i). The reference product informs structure only, never its name,
logo, screenshots or copy.

## What cannot be validated without a real certificate, a real gateway account and an accountant

The exit criterion of the roadmap says "authorized documents in the provider's test environment". **That is not provable
in CI and this ADR does not pretend it is.** The gateway requires, **even in homologation, a real CNPJ and a valid A1
certificate**. What the tests prove is the product's side: a product sale and a service sale produce authorized documents
**against the fake gateway and against a local stand-in of the real adapter's HTTP shape, on both storage adapters and through
the API**, and their XMLs reach the accountant's package. The ignored sandbox test (`FOCUS_NFE_SANDBOX_TOKEN` and the issuer's CNPJ,
like [ADR 0049](0049-asaas-provider.md)'s) is the homologation and **only the user can run it**. The correctness of every tax code
and rule is the accountant's (slice 8's checklist) and is not validated by anything here.

## Consequences

- The product issues fiscal documents only through a gateway; without the environment it still works for everything else and
  the fiscal page says why it cannot issue.
- A company's certificate and its password are never in our database, logs or memory; the cost is that the person uploads it
  at the gateway, and that expiry is the person's date, not read from the certificate.
- One issuing CNPJ per installation, until credentials may be encrypted at rest.
- Fiscal files live in the `BlobStore`, count toward the package's limits and are never deleted.
- Every service that writes a fiscal record asks the period lock; cancelling or invalidating in a closed month needs a reopen.
- No tax rate or legal table is embedded; wrong data in the company's rules is the company's, but the product makes no check
  that the codes are right for the regime beyond their shape.

## Revisit when

An installation must issue for several CNPJs (credentials encrypted at rest); the certificate must be uploaded through the
product (the master-token question answered); a salesperson must issue without the `financial` role; the gateway documents a
signature for its notices; or the consumption-tax reform (IBS/CBS) changes the documents' layouts ([ADR 0006](0006-fiscal-last.md)).
