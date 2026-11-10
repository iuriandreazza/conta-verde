---
title: "ADR 0082: NFC-e at the counter and the minimal point of sale"
status: accepted
date: 2026-11-10
tags: [adr, fiscal, phase-7, nfce, pos]
---

# ADR 0082: NFC-e at the counter and the minimal point of sale

## Context

[ADR 0073](0073-fiscal-module-plan.md) puts the NFC-e (the consumer's invoice, model 65) after the NF-e and the NFS-e, with "a minimal
point of sale". [ADR 0077](0077-choosing-a-fiscal-gateway.md) says every fiscal call is built for the fake, Focus NFe and NFE.io.
The NFC-e has the NF-e's identity (a 44-digit access key whose model is 65, a number and a series) and, at Focus NFe, is **answered at
once** (`201` authorized or refused in the same call, where the NF-e is answered later).

## Decision

### The kind and its identity

`FiscalDocumentKind::Nfce` takes the NF-e's columns, rules and `CHECK`s (a migration widens the two that name the kinds), has its own
series (`nfce_series`) and is numbered by the gateway. **Invalidating a range of numbers stays the NF-e's**: `number_is_used` looks only
at NF-e documents, so a number only an NFC-e holds does not stop an NF-e's invalidation (and the port has no NFC-e invalidation). An NFC-e
has **no correction letter** (`correct` refuses it), is cancelled with a reason of 15 to 255 characters like the others, and the
accountant's package carries it as `fiscal/nfce/{key}` in the fixed columns.

### What the person chooses, and what is built

`IssueNfce` has no default: the nature (a product nature), the origin of the goods (0 to 8), how the buyer was present, whether the consumer
is **named on the invoice** (the sale's customer with their CPF or CNPJ; a customer without a document cannot be named, and not naming
leaves a recipient called "Consumidor não identificado" with no document) and **what is paid, per form**. The invoice covers the **product
lines** of the sale (a service line is the NFS-e's), taxed as the nature says for a sale **inside the state** (the CFOP is the nature's for
the issuer's state), under the Simples Nacional only. **What is paid must add up to the total of the invoice: no change is given**,
because Focus NFe's documentation does not say how change (`troco`) is declared, and a total that did not add up would be refused by the
authority. The code refuses a payment that is short or over, a payment of 0 and an amount with more than two decimals before anything is sent. The buyer's presence is one the counter has (in person, delivery, outside the establishment or other: **unverified against the authority's rule**, recalled from the layout and to be confirmed), and the authorized document's key must be of the kind's model (55 or 65).

### The port, the fake, Focus NFe and NFE.io

`submit_nfce`, `query_nfce` and `cancel_nfce` with the NF-e's outcomes, and `issues_nfce` (true by default) which the service asks **before
writing anything**, as it does `issues_nfse`. The fake numbers per issuer, environment, series **and model**. Focus NFe (`/v2/nfce`) is
built from its documentation and answers synchronously; **its printed page (the DANFCe) is HTML, not a PDF, so it is not kept**: the XML
is the record, the file kinds stay `xml`, `pdf` and `cancellation_xml`. The body is the documented required fields and the items of the
NF-e; nothing undocumented is sent (no change, no contingency).

**NFE.io answers `Unsupported` and sends nothing**, and this time for a different reason than the NFS-e's: it has an API for consumer
invoices (`consumerinvoices`, its own OpenAPI file, with the same listing by `q` and queued cancellation as its product invoices), but
the schema of its request was **not read** in this slice, and an NFC-e is not sent on a guess. An installation that issues NFC-e uses Focus
NFe (or the fake) until that adapter is written; this is the second place ADR 0077's "built for both" is not met.

### The minimal point of sale

The **Caixa** page (`/companies/{id}/pos`, for whoever writes `sales` and `fiscal`) sells products and invoices them in one go **through
the endpoints that exist**, one after the other in the browser: the sale (`POST /sales`), its completion, and `POST
/fiscal/consumer-documents`. The sale is made once, completed once and invoiced until the invoice is authorized: a step that fails is the one asked again (a
completion refused for want of stock leaves an open sale, completed on the next try), the lines stop being editable once the sale exists,
and an invoice the authority refused is **sent again under its own id** (`resend-consumer`), never issued as a second one. A lost
response to the creation of the sale can still make a second sale on retry (there is no idempotency key). The sale is dated today where the person is (the server reads no clock), and the lines use the shared editor with the price of the
product. It does not print: the XML is in the documents list. The Documentos page lists NFC-e with their kind and cancels them; a
**rejected NFC-e has no resend screen in Documentos** (the Caixa resends it right after the refusal; the API has `POST .../resend-consumer`).

## Consequences

- A product sale can be invoiced three ways (NF-e for a company, NFC-e at the counter, NFS-e for services), each document with its own
  total, and a sale has at most one live document of each.
- **Not validated without a real CNPJ, certificate and the CSC token** (Focus NFe keeps the CSC; the documentation does not say where it is
  set, and answers `empresa_nao_configurada` when it is not): the body, the answer's fields, the cancellation window of the state, and
  anything of NFE.io.
- Contingency (offline) is not issued by the product: the documentation gives `forma_emissao=offline` but not how it is made effective.

## What this does not decide

The NFE.io consumer adapter; change and offline contingency; printing the DANFCe; a resend screen; the Lucro Presumido regime (slice 8).
