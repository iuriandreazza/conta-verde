---
title: "ADR 0083: Received invoices, the recipient's manifestation and the fiscal webhook"
status: accepted
date: 2026-11-10
tags: [adr, fiscal, phase-7, mde, webhook]
---

# ADR 0083: Received invoices, the recipient's manifestation and the fiscal webhook

## Context

[ADR 0073](0073-fiscal-module-plan.md) puts the recipient's manifestation (*manifestação do destinatário*, MDe) of incoming NF-e in
the fiscal module, and a webhook for the gateway to say that something changed. The invoices issued **against** the company are not the
company's data: the authority holds them, the gateway lists them, and the company may answer to the authority about each with awareness,
then one final word (confirmation, unknown, operation not performed). The webhook of a gateway carries a payload this product cannot
verify, and Focus NFe's documentation gives neither its structure nor its retry policy.

## Decision

### Received invoices are a mirror of the gateway's list

`ReceivedInvoice` (domain, `received_invoices` table): the access key, the issuer's name and document, the total, the day, the
**situation** (`authorized`, `cancelled`, `denied`), the **manifestation** and the gateway's **version**. A listing asks "newer than the
biggest version we hold" (`versao` at Focus NFe), a page at a time (the use case asks again from the biggest version it got, up to 20
passes), and mirrors what it gets: a new invoice is added, one the company has is brought up to what the gateway says. **The mirror is not
audited** (the way a payment notice is a log): it is the gateway's. A record the adapter cannot read makes the whole listing
`Unavailable` instead of being skipped, because the versions after it would hide it for good. The rows live in the
`FiscalDocumentRepository` port (both adapters, one contract: `received_invoices_are_kept`) because they are fiscal records of the
same module and the port already has both adapters.

### The words, and who says them

The rules of the words are the domain's (`Manifestation::may_become`): awareness from nothing; a final word (confirmation, unknown,
operation not performed) from nothing or from awareness; nothing after a final word; only an **authorized** invoice is answered; only
"operation not performed" has a reason, of 15 to 255 characters on one line, and it needs one. They are checked **before** the gateway is
asked. A word is declared through the gateway (`POST /v2/nfes_recebidas/{key}/manifesto`, answered at once); **only when the authority took
it** is it recorded, in the name of the person (`AuditEntity::ReceivedInvoice`, with the word it replaced, and the written-to row updated
under a lock only from the word the person saw: `FiscalDocumentChanged`). The authority's refusal is told with its words and records
nothing. **A word is not idempotent**: an answer that never came is not sent again on its own; the person fetches the list and sees what
the authority holds (a duplicate would be refused by the authority anyway). A gateway listing says its word again, and the mirror takes
it, dropping a reason that belonged to another word.

### Gateways

`list_received_nfe`, `manifest_received_nfe` and `issues_manifestation` on the port. The fake holds what a test makes a supplier issue
and gives two per page. **Focus NFe** from its documentation (`GET /v2/nfes_recebidas?cnpj=&versao=`, `POST .../manifesto` with `tipo`
`ciencia|confirmacao|desconhecimento|nao_realizada`): the fields read are `chave_nfe` (with the `NFe` its documentation writes before
the key), `nome_emitente`, `documento_emitente`, `valor_total`, `data_emissao`, `situacao`, `manifestacao_destinatario` and `versao`; its
`evento_registrado` is the authority taking the word, `erro` or a `4xx` a refusal. **NFE.io answers `Unsupported` and sends nothing**: its
distribution of received invoices is another API, not written in this adapter (the third place where ADR 0077's "both" is not met, stated
here and in the PR). **Unverified**: that the company's CNPJ is enabled to receive at the gateway (nothing says so in the
documentation), the values of `pendente`, whether `versao` pages as the product assumes, and the rate limit.

### The webhook wakes the server up and nothing it carries is believed

`POST /public/fiscal/{company_id}/webhook`, **no session**, only when the operator sets `FISCAL_WEBHOOK_SECRET` (at least 32 characters;
the server refuses a shorter one) and optionally `FISCAL_WEBHOOK_HEADER` (a header name, `x-fiscal-webhook-token` by default): what is
registered at the gateway as the hook's `authorization_header` and `authorization`. The secret is compared in constant time and never
logged. `404` when no secret is set, `401` for a wrong or missing one, `404` for a company with no fiscal settings (and for an id that is
no company), body over 64 KiB refused, and then **the body is read for nothing**: the server asks the gateway for what is new
(`sync_received`) and runs the poll of the waiting documents, exactly what it does anyway, answering `202` whatever the asking finds (a
failure is the poll's to retry). The poll reaches every company (it is the same one the minute pass runs); the knock only makes it run
now. There is no throttle on valid knocks: the secret is the gate.

### Screens

**Notas recebidas** (`/fiscal/received`) lists the mirror, narrows by what was declared, fetches on request ("Buscar notas") and offers
only the words that may follow, asking for the reason only where it is needed. A reader sees the list and nothing to do. There is **no
scheduled sync**: the knock and the button are the two ways (a pass over every company would need a list of companies that issue).

## Consequences

- The recipient's manifestation is a fiscal act of the person, in the trail, never made by the system on its own.
- Registering the hook at the gateway is the operator's (its panel or `POST /v2/hooks`): the product does not call it.
- **Not validated without a real CNPJ enabled at the gateway and the authority**: the list's shape, the paging, the words' acceptance.

## What this does not decide

The NFE.io adapter; a scheduled sync; downloading the received invoices' XML and DANFE (the gateway has them; the mirror keeps no
files); the webhook's payload (never read); the manifestation of a CT-e.
