---
title: "ADR 0081: NFS-e documents, the DPS numbering and the port's NFS-e calls"
status: accepted
date: 2026-11-10
tags: [adr, fiscal, phase-7, nfse]
---

# ADR 0081: NFS-e documents, the DPS numbering and the port's NFS-e calls

## Context

[ADR 0080](0080-nfse-plan-and-settings.md) put what the issuer and a service nature must say into the settings and left three things
to this slice: the numbering of the DPS (*declaração de prestação de serviço*), how each gateway names, cancels and answers an NFS-e,
and the document itself. [ADR 0077](0077-choosing-a-fiscal-gateway.md) says every fiscal call is built for the fake, Focus NFe and
NFE.io, with the shared contract. The NFS-e is not a copy of the NF-e:

- it has **no 44-digit access key** (the national standard has its own key, a number and a verification code, and a gateway for municipal
  layouts may give only the number);
- in the national standard the **issuer numbers the DPS** (series and number), where the gateway numbers an NF-e;
- it is the document of **services**, so a sale of products and services makes two documents;
- the national NFS-e has **no correction letter and no invalidation of numbers**.

## Decision

### An NFS-e is a fiscal document of its own kind

`FiscalDocumentKind::Nfse` joins `Nfe`; everything ADR 0076 says of a document (statuses, our id as the gateway's reference, the content
frozen once authorized, one live document per sale **and kind**, files in the blob store, the poll) holds for it. What the authority gave is
**not** an `AccessKey`: it is an `NfseAuthorization` (the number, and the national key and the verification code when the gateway gives
them), with its own columns (`nfse_number`, `nfse_key`, `nfse_verification_code`), a `CHECK` that each kind has its own and none of the
other's, and a unique index on the key per company and environment. The NF-e's `access_key`, `number` and `series` columns, its unique
index and the invalidation of numbers stay the NF-e's: an NFS-e number never counts as a number "in use" of an NF-e series. The trail
records the new fields, and the Postgres trigger that keeps an authorized document's content fixed knows them.

### The DPS is numbered by the company, once, and never reused

`Dps { series, number }` (series 1 to 49999, the range the Focus API documents for an API issuer; number up to fifteen digits) is taken
**before the document is sent**: the next number is one more than the biggest the company's documents hold in that series and environment
(`FiscalDocumentRepository::next_dps_number`), written with the document in the same transaction as the move to `numbered`, under a unique
index on (company, environment, series, number). Two writers given the same number: the second is refused (`DuplicateFiscalDps`) and asks
again. The number is then kept through **every new attempt**: a rejected document is sent again under the same id and the same number, a
gateway that was down leaves the document `processing` with its number, and a cancelled document's number is not given again. The series is
the settings' `nfse_series`. Homologation and production count apart.

**Unverified:** whether the national system lets a rejected DPS number be sent again corrected, or burns it. The product assumes it can be
resent, because the alternative (a new number per attempt) would lose the link with the document and leave gaps with no document to explain
them; if the sandbox says otherwise, a resend answers the old refusal and the rule changes in a new ADR.

### What the person chooses, and what the nature says

`IssueNfse` has **no default**: the operation nature, the competence day, the municipality where the service was rendered (an IBGE code of
seven digits: it decides whose ISSQN it is and is often not the issuer's) and the special tax regime. The ISSQN terms are the **nature's**
(ADR 0080) and the number, the municipality code of the issuer and the series come from the settings; nothing is calculated. An NFS-e is
refused before anything is sent when the regime is not the Simples Nacional (the tax calculation of the others waits for slice 8), the
issuer has no municipality code, the nature is not an active service nature, the nature has no ISSQN terms (the message names it), the
customer has no CPF or CNPJ, or the sale has no service line. A customer that is a company is accepted (an NFS-e does not depend on a state
registration). The Simples option sent to the gateway is derived from the regime: an optant (3, ME/EPP, since the settings refuse a MEI).

### One sale, two documents: each covers its own lines

The NFS-e covers the **service** lines of the sale: the description is the lines' descriptions joined by `; ` (refused over the 1000
characters the layout takes, never cut), the amount is their gross, the unconditional discount their discounts and the document's total is
what they are worth. The NF-e now covers the **product** lines only, with its own total (it refused a sale with a service line before).
Asking for each is independent.

### The port: three calls, no more

`submit_nfse`, `query_nfse` and `cancel_nfse`, with `DocumentOutcome<D>` and `Submission<D>` made generic over what an authorized document
hands over (the NF-e's by default, so no NF-e call site changed). There is **no correction letter and no invalidation**: the national
standard has neither, so the port does not carry methods that only answer `Unsupported`, and `correct` on an NFS-e is refused with that
reason. A cancellation of an NFS-e is answered at once (never `Queued`), and the poll leaves queued events to NF-e documents.

### Focus NFe

Built from the documentation published at `doc.focusnfe.com.br` (the national endpoint `/v2/nfsen`): `POST ?ref=` with the DPS, `GET
/{ref}`, `DELETE /{ref}` with `justificativa`. It sends the fields the documentation lists as required and the nature's own, **none of the
tax reform's fields, and nothing invented**. The answer's `numero`, `codigo_verificacao` and the XML are taken; the printed form
(`url_danfse`) is downloaded **only when it is on the gateway's own host**, since the documentation says the URL may be a storage service's
and a file elsewhere is never followed (ADR 0075), so an NFS-e may be kept without its PDF (the XML is the record). The national key is not an
answer field of the documentation, so it stays empty. A refusal of the request (`400` or `422`) is told from "this reference is already
known" by **asking**: a document under the reference is the answer, none is a refusal, since the documentation gives no code for a
duplicate. Tested against a local stand-in; the sandbox test needs a real CNPJ and certificate.

**Unverified, for the sandbox to settle:** which of the documented fields the service really requires (the documentation marks the
reform's among them and says not all are mandatory), the types of the coded fields (it writes them as strings and the example as numbers;
numbers are sent), the format of `data_emissao`, the tomador's municipality code (not sent: it is optional and the registers keep no
IBGE code of a customer), whether the same reference is accepted again after `erro_autorizacao`, and the meaning of the status `negado`
(taken as a denial).

### NFE.io: not built, and why

NFE.io's NFS-e (`serviceinvoices`) is **not** an adapter in this slice, and not because the gateway lacks it: its published reference does
not give what ADR 0078 and ADR 0079 require before an adapter may submit, which is a way to **find what it made for our reference** so
that a lost answer never issues twice. The reference reads, for the listing, a period and a page and no filter by our reference or by
`externalId` (one page says `externalId` serves idempotency, the SDK reference lists no such field), and it does not document the header of
the key, the host of this product (it names `api.nfe.io` and `api.nfse.io`), the answer's fields or the values of `flowStatus`, nor whether
a cancellation is answered at once. Until a file that settles those points is read, the three calls answer `Unsupported` with the reason
and **send nothing** (tested: no call reaches the gateway). **An installation that issues NFS-e uses Focus NFe or the fake for now**; this
is the one place where ADR 0077's "built for both" is not met, and it is stated rather than guessed.

### The accountant's package

An NFS-e issued in production is in the package like an NF-e, **in the same fixed columns** (ADR 0076 does not change): the number, the
DPS series in the series column, the national key in the key column (empty when the gateway gave none), and the files under
`fiscal/nfse/{key}` (or `{number}` when there is no key). The verification code is not a column.

## Consequences

- An authorized NFS-e's identity is not an access key anywhere: the API sends `nfse` (number, key, verification code) and `dps` and leaves
  `access_key`, `number` and `series` empty for it; the web's `FiscalDocument` type has both.
- A sale of services and products is invoiced twice, and each can be cancelled alone.
- **What cannot be validated without a real certificate, a gateway account and a municipality that joined the national standard:** that
  Focus NFe's national endpoint accepts this body, the numbering rule above, the cancellation window of a municipality, and anything of
  NFE.io. The tests prove the product's rules on both storage adapters and through the API, against the fake and a local stand-in.

## What this does not decide, and is left

The screens (slice 5c); municipal (non-national) layouts; the tax calculation of the other regimes (slice 8); the NFS-e through NFE.io.
