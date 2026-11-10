---
title: "ADR 0078: A reference the gateway assigns and events it answers later"
status: accepted
date: 2026-11-10
tags: [adr, fiscal, phase-7]
---

# ADR 0078: A reference the gateway assigns and events it answers later

## Context

[ADR 0077](0077-choosing-a-fiscal-gateway.md) found two things in NFE.io's public specification that the `FiscalGateway` port of
[ADR 0075](0075-fiscal-gateway-port.md) could not express, and that made an adapter for it unsafe: the gateway **assigns the id of
the invoice** (no client reference, no documented idempotency), and **cancellation and correction letter are asynchronous**
("queued", the result comes later). This ADR changes the port, the document and the events so that **both shapes are supported**;
Focus NFe is unaffected (it keeps our reference and answers events at once).

## Decision

### The gateway's reference

- `submit_nfe` returns a **`Submission { reference: Option<String>, outcome }`**. A gateway that keeps our reference (Focus NFe)
  answers `None`. A gateway that names its documents answers the name it gave.
- The **document stores it** (`fiscal_documents.gateway_reference`, `FiscalDocument::referenced`): set **once**, only while the
  document waits, **never changed** (domain, both adapters and a trigger), and part of what is frozen once it is authorized. It is
  written **before** the outcome is applied, in its own audited step, because every later call is about the document by that name.
- Every later call (`query_nfe`, `cancel_nfe`, `correct_nfe`, `query_event`) takes **the gateway's reference when there is one,
  ours otherwise**.
- **Never issue twice.** The danger is a name lost between the gateway accepting a submission and the document storing the name:
  the next submit would make a second invoice. The port's contract is therefore that **an adapter for such a gateway finds what it
  already made for our reference before it submits, and does not submit when it cannot tell** (`Unavailable`). Our reference travels
  in the request so that it can be found. The fake implements this behavior (`assigning_references`) and the contract test runs it
  on both storage adapters. **How a real gateway can be searched is the adapter's own ADR**, and is unverified until a sandbox call.

### Events answered later

- `EventOutcome` gains **`Queued`**: the gateway took the request and will do it later. A new port call **`query_event(issuer,
  reference, EventQuery)`** asks again (`Cancellation`, or `Correction { accepted_before }`: how many letters the authority had
  already accepted for the document, because attempts that were refused never count in its sequence). A gateway whose events are
  synchronous never queues one and its `query_event` is `Unsupported`.
- **The cancellation becomes an event**, like the correction letter and the invalidation ([ADR 0076](0076-nfe-documents.md)): it
  is **recorded as `sent` before the gateway is asked** (`fiscal_events`, kind `cancellation`, the reason as its text) and settled
  once. When the gateway answers at once (`Accepted`), the event is settled, the cancellation's XML is stored with the document
  and the **document becomes `cancelled`**, as before; a refusal settles the event as refused (`422` with the authority's words)
  and the document stays authorized. When it answers `Queued`, **the event stays `sent` and the document stays `authorized`**.
- **At most one cancellation is open per document**: waiting or accepted (a unique partial index and the use case); a refused one
  does not count, so the person may ask again.
- **A pass settles what was queued.** `FiscalDocumentService::settle_queued_events` takes the `sent` correction letters and
  cancellations of every company, oldest first (up to the pass's limit), asks `query_event`, and settles each in the name of
  whoever asked: a letter takes the authority's XML, a cancellation moves the document to `cancelled`. It runs inside the existing
  once-a-minute poll (`poll_pending`). A queued event that never gets an answer is given up by the person (`abandon_event`), as
  any other `sent` event. A webhook route (slice 7) would only make the answer come sooner.
- The document's answer carries **`cancellation`** (the waiting or accepted one), and the Documentos page says that the
  cancellation is waiting, offers no second one and lets the person give up on it.

## Consequences

- Focus NFe behaves as before; only the contract changed shape (a `Submission` around the outcome).
- A document made by a gateway that names its documents cannot be asked about by another gateway: **a company that changes gateway
  finishes or cancels what is waiting first** (ADR 0077).
- The pass reaches every company and has no age window for events, so more than its limit of stuck events would hold newer ones
  back; the person gives up on a stuck one.
- Nothing of this proves a real gateway does what the fake does: it proves the product's side of the contract.

## Revisit when

A gateway documents a signed notification (a webhook route settles events at once); or an event needs a time limit.
