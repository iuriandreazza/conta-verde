---
title: "ADR 0076: NF-e documents: the store, issuing from a sale, events and the package"
status: accepted
date: 2026-11-10
tags: [adr, fiscal, phase-7]
---

# ADR 0076: NF-e documents: the store, issuing from a sale, events and the package

## Context

[ADR 0073](0073-fiscal-module-plan.md) planned the document, its states and how issuing works; [ADR 0074](0074-fiscal-settings.md)
holds the issuer and the natures; [ADR 0075](0075-fiscal-gateway-port.md) is the port. This slice makes the product issue an NF-e from
a sale. It is **too large for one reviewable change**, so it ships as **three pull requests inside the slice**, each mergeable on
its own, and the roadmap item "NF-e" is ticked only when the third lands:

| PR | What it holds |
|----|---------------|
| **4a, the store** (merged) | `FiscalDocument` and its state machine in `domain`, the `FiscalDocumentRepository` port, both adapters, the migration, files in the `BlobStore` (`BlobId`), contract tests |
| **4b, issuing** | `FiscalDocumentService`: issue from a sale, refresh, send again, cancel (with the period lock), download the files; the poll; the routes under `/fiscal/documents`; the certificate and Simples-only checks |
| **4c, events** | the correction letter and the invalidation of numbers: `fiscal_events`, `FiscalEvent`, record-before-send, the routes |
| **4d, the package** | `fiscal.csv` and the XML/PDF/event files in the accountant's package, within its limits |
| **4e, the screens** | the list endpoint, the documents page and its actions |

This ADR records the decisions of all four so that each pull request can be read against it; the sections say which pull request
carries what. (It was three pull requests when this was first written; the events were split from issuing because each is a
review of its own.)

## Decisions

### What a document keeps (4a)

A `FiscalDocument` keeps what a person must be able to read years later and what the accountant's package needs: its **kind**
(`nfe`; `nfse` and `nfce` join with their slices), its **origin** (a sale), the **issuer snapshot** (CNPJ, legal name, regime,
environment), the **recipient's name and document**, the **nature** it was issued under, the **total**, the **moment** it was
issued (Unix seconds, the caller's) and the **issue day**, the status, the refusal (code and text) of a rejection or denial, the
**authorization** (access key, number, series, protocol), who submitted it and a note. **It does not keep the lines**: they are in
the XML the authority signed, which is the fiscal record, and a second copy in our tables would be one more thing to disagree with
it. ADR 0073 said the lines are kept "as sent"; this ADR narrows that to the totals and the XML. The cost: a rejected document that
is sent again is **built again from the sale** (the person fixes the sale or the registers first), never replayed from a stored
copy.

States and moves are as in ADR 0073. The transitions are methods of the domain type that return a new value or refuse
(`InvalidFiscalTransition`): `Draft` or `Rejected` to `Processing` (submitted), `Processing` to `Contingency`, `Rejected`, `Denied`
or `Authorized`, `Contingency` to the same three, `Authorized` to `Cancelled`. Nothing leaves `Denied` or `Cancelled`.

**Content freezes at authorization.** Once a document is `authorized` (or `cancelled`) the adapters refuse to write its content
(origin, issuer, recipient, nature, total, moment, authorization), only its status; **Postgres also refuses it with a trigger**, so a
bug in a use case or a hand-written statement cannot rewrite a fiscal document. Only a `draft` or `rejected` document has its
content replaced (when it is sent again).

**One live document per sale and kind**: a partial unique index on `(company_id, sale_id, kind)` where the status is `draft`,
`processing`, `authorized` or `contingency`, and the in-memory adapter refuses the same case (`FiscalDocumentAlreadyLive`), proved
by one contract test on both. A `rejected`, `denied` or `cancelled` document no longer holds the sale. A rejected one is sent again **under the same id and
reference** (once the person fixed the sale); if the sale has meanwhile been given another live document the write is refused
(`FiscalDocumentAlreadyLive`). A denied or cancelled one is final and a new document gets a new id. **A cancelled or denied
document never moves again**, held by the domain, both adapters and the trigger. An access key names one document of a
company, also held by both adapters.

### Files (4a)

The XML and the DANFE PDF are stored in the `BlobStore`, never in the database. The store was keyed by `AttachmentId`; it is keyed by
a **`BlobId`** (a UUID) now, and an `AttachmentId` converts into it, so an attachment's file is at **the same place on disk** as
before (`<root>/<company>/<id>`; a test writes with the attachment's id and reads with the blob's). A `fiscal_files` record
(company, document, kind, blob, size, SHA-256) says whose a file is and what it holds. **The file is written before the record that
names it**; a file is **never deleted**, not even when its document is cancelled (the XML of the cancellation is another file).
Kinds in 4a: `xml`, `pdf`, `cancellation_xml`; 4b adds the correction letters' XML with a migration that widens the `CHECK` and the
key (one file per document and kind becomes one per document, kind and number). A file is stored once per document and kind: a
second one is refused (`DuplicateFiscalFile`), so an authorized document's XML never changes; the use case that finds a file
already recorded does not write it again.

### Issuing (4b)

- **Who may**: the `fiscal` area; the person's name is in the audit trail and on the document (`submitted_by`).
- **The moment** is passed by the caller in Unix seconds; the **issue day** is that moment in the **issuer's state** (`fiscal_timestamp`,
  ADR 0075), and **that day is what the period lock judges** (ADR 0073). The server's clock is read only to make the moment, in the
  API layer, as for document mails.
- **What is refused before anything is sent** (`422` with the reason, naming the line or field): no fiscal settings; a gateway that
  is off; the certificate day passed (a comparison of the settings' day with the issue day); a regime the slice does not issue for
  (only Simples Nacional, ADR 0073); a sale that is not `completed`; a product with no NCM; a customer with no document or no
  address; a nature that does not exist, is inactive or is not a product nature; an item that no tax rule covers.
- **The CFOP**: the nature's CFOP for a sale **inside the state** when the recipient's state is the issuer's, else the one **for
  another state**; a recipient with no address is refused (the destination cannot be known, and guessing it is a tax fact).
- **The tax rule for an item** is the one ADR 0074 defines: of the company's regime, the longest NCM prefix the item's NCM begins with,
  else the nature's general rule. A nature with no rule for the regime refuses the issue.
- **The product's origin** (0 to 8) is not on a product nor on a rule, and **a default would be a tax fact the product made up**.
  The issue request carries one `origin` for the whole document, which the person chooses; there is no default. **Verify with an
  accountant** that one origin per document suits the company's products (a document with goods of two origins needs a product-level
  field, which is a later decision).
- **Only a customer with a CPF is invoiced for now.** A company that buys may be a taxpayer of the ICMS, which changes how the
  recipient is declared, and the registers do not keep a recipient's state registration: the adapter declares a recipient with none
  as a non-contributor, which would be a tax fact the product made up for a CNPJ. A recipient with a CNPJ is refused (`422`)
  until the party register keeps the state registration (a later slice). The freight mode of the document is "no freight" and
  there is no field for transport yet; **verify with an accountant** that this suits the company's sales.
- **Buyer presence** is also chosen by the person in the request (ADR 0075); there is no default.
- **Stepwise and repeatable**: (1) validate and write the `draft`, whose unique index is the claim, so asking twice finds the same
  document; (2) mark it `processing` and `submit` with the document's id as the reference; (3) store the outcome and, if authorized,
  the files. A failure after any step is finished by asking again. `Unavailable` leaves the document `processing` (the gateway may
  have taken it) and the poll or the person's refresh asks the state, **never a second submit with another reference**.
- **`Unknown` from a query of a reference we submitted** means the gateway has no document under it (the submission never
  reached it, or it was lost): the document is marked **rejected** with the code `unknown` and a text that says to send it again,
  and the person sends it again **under the same id** (`POST .../resend`, which builds it again from the sale). Never automatic,
  and never under another reference. The same path serves a submission that failed with `Disabled`, `WrongIssuer` or
  `Unsupported`: nothing was sent, so the document is rejected with the code `gateway` and the error's words.
- **The poll is a pass, not a job.** ADR 0073 planned a `FiscalPoll` job on the queue. A pass over what waits is simpler and
  cannot lose a document: `FiscalDocumentRepository::pending(since, limit)` lists the documents of every company that are
  `processing` or `contingency` and were issued in the last seven days, oldest first, and `FiscalDocumentService::poll_pending`
  refreshes each one, once a minute, from a task in `main.rs` (like the recurrences, ADR 0033: every step is harmless to repeat, so
  overlapping passes and restarts do no harm, and nothing has to be queued by whoever submitted). A document older than a week
  is left for the person's refresh (`POST .../refresh`). What the poll writes is in the name of **whoever submitted the
  document**. A document that authorizes through the poll or a refresh is written **whatever the period lock says, with a note**.
- **"Unknown" waits ten minutes.** A refresh, or the poll, believes "the gateway has no document with this reference" only for a
  document issued at least ten minutes before (`UNKNOWN_GRACE_SECONDS`), so a submission that is still on its way is not refused
  under the feet of the call making it. A refresh or a cancel also **refuses to ask when the company's issuer CNPJ or environment
  differs from the document's** (a document of homologation is not asked about in production): put the settings back first.
- **Asking again after a rejection makes a new document.** `POST .../fiscal/documents` for a sale whose only document is rejected
  creates a new one with a new id and reference; **`resend` is the way to keep the id**. Both are safe because a rejected document is
  one the gateway never authorized (a gateway that did authorize it answers the query, which is why a document is rejected for
  `Unknown` only after the grace above). Asking for a sale that already has a live document, in a month that has since been
  closed, still returns that document: the lookup comes before the certificate and the lock.
- **The poll's limits**: it takes the oldest 50 waiting documents of the last seven days each minute, with no rotation, so
  more than 50 stuck documents would hold newer ones back until they leave the window; the person's refresh is the way past it.
- **The audit trail**: every move is an `AuditEntry` of the document, written in the transaction that stores the move, in the name of
  whoever submitted it for what the poll does.

### Events and invalidation (4c; cancel is 4b)

- **Cancel**: queries first (a gateway that already cancelled it says so and the product follows), then asks the gateway with the
  reason (15 to 255 characters, checked by the domain before the call), and marks the document `cancelled` with the cancellation's XML.
  A refusal (`Refused`) is `422` with the authority's text and changes nothing. **Closed month: `409 PeriodLocked`**.
- **Events are recorded before the gateway is asked** (`fiscal_events`, one table, `FiscalEvent` in `domain`): a row with status
  `sent` is written, then the gateway is called, then the row is **settled once** to `accepted` (with the protocol and the
  authority's XML) or `refused` (with the authority's words). A settled event never changes, and no event is deleted (a trigger
  holds both). Its content (what was asked, by whom) is never written again by an adapter; only the answer is.
  **An answer that never came (or an accepted one the product could not finish recording, for instance a file that could not be downloaded) leaves the event `sent` and the product never sends it again** (ADR 0075: events are not idempotent
  at the authority); the person sees it and the gateway's own record is the truth. Only the person can give up on such an event (`POST .../fiscal/events/{id}/abandon`, after checking at the gateway that the authority did not take it): it is settled as refused with the code `abandoned`, which frees a new letter. **Never for an event the authority may have accepted** (the product cannot tell). The XML of an event is kept in the `BlobStore`
  under the event's own `blob_id` (not in `fiscal_files`, whose rows are a document's), downloaded at
  `GET .../fiscal/events/{id}/file`.
- **Correction letter** (`POST .../documents/{id}/corrections`): only for an `authorized` document; text of 15 to 1000
  characters on one line; its place is `1 + the attempts so far` (at most 99, ours alone), and the **authority's limit of 20 counts the accepted ones**
  (the unique index on `(company, document, sequence)` means two writers cannot both take number 3; the loser is told it
  changed); **a new letter waits while one is still `sent`**. The authority's deadline (720 hours) and rules are its own to
  enforce; a refusal is kept and told (`422` with the code and text). A letter changes no figure, so a closed month does not
  refuse it. Nothing here asks the authority which sequence number it has: the gateway assigns it.
- **Invalidation** (`POST .../fiscal/invalidations`): a series (1 to 999), a range of numbers (first to last, at most
  999,999,999) and a reason of 15 to 255 characters, in **the environment the company issues in now**. **A number a document of
  the company holds in that series and environment (authorized or cancelled) is never invalidated** (`422`): that is the rule
  that stands in for the period lock, since a range has no document of its own to judge by and an invalidation of a number that
  was used would be wrong at the authority too; a range none of whose numbers we hold changes no figure of any closed month.
  (**Verify with an accountant** whether a closed month should also refuse an invalidation of numbers the company never used.)
  The company's invalidations are listed newest first, by the order they were recorded in (the latest 100). **Limit:** a number held by a document still `processing`, in `contingency` or `denied` is not stored, so it is not seen as used; asking the authority is the check that holds.

### The package (4d)

As ADR 0073 amended ADR 0071, the package also holds **`fiscal.csv`** (always a CSV, with its header when nothing was issued) and
the files under **`fiscal/nfe/`** for the documents **issued in the span** (the issue day, both ends in) in **production**
that are authorized or cancelled: `{access_key}.xml`, `{access_key}.pdf` when the gateway made one, `{access_key}-cancel.xml`
and `{access_key}-cce-{n}.xml` for each accepted letter, `n` being the letter's place among our attempts. **A document of
homologation is never in the package** (a test document is not a fiscal document), and a rejected or denied one has no files.
The columns of `fiscal.csv` are fixed and do not change without a new ADR: `document_id`, `kind`, `status`, `number`,
`series`, `access_key`, `issued_on`, `recipient_name`, `recipient_document`, `total`, `environment`, `nature`, `xml_path`,
`pdf_path`, `cancellation_path` (a path is empty when that file does not exist; the letters are files, not columns).
The files **count toward the package's limits** (500 files and 200 MiB with the attachments, 50,000 rows), judged from the
records **before any file is read**, and a span that does not fit is `422`, never cut. `GET .../accounting/exports/package` makes
the fiscal part from `FiscalDocumentService::package`, given what the attachments left of the limits, and the Contador page
says so.

### The documents page (4e)

The list endpoint and the **Documentos** page (list, issue from a sale, detail with the files, cancel, correction letters and
invalidations) are the last slice of the NF-e, with their own pull request.

## What this cannot prove

Everything of ADR 0073's last section holds: the fake and the local stand-in prove the product's side, and **only a real
certificate, a gateway account and an accountant can prove an authorized document, a tax code or a deadline**. The documents of
homologation are not fiscal documents.

## Consequences

- A rejected document is rebuilt from the sale, not replayed: a person who fixes the sale gets a fresh document.
- One origin and one buyer presence per document, chosen by the person: more typing, no invented tax fact.
- A fiscal document and its files can never be rewritten or removed by the product.
- Three pull requests where the plan said one: the roadmap item stays unticked until the third.

## Revisit when

A document needs goods of several origins; the lines must be shown without opening the XML; the invalidation's closed-month rule is
decided by an accountant; or a gateway documents a reliable way to ask whether an event was taken.
