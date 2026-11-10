---
title: "Homologation test plan for the fiscal documents"
status: draft
date: 2026-11-10
tags: [fiscal, homologation, test-plan, nfe, nfce, nfse]
---

# Homologation test plan for the fiscal documents

**Nothing in the fiscal module has ever been exercised against a real gateway or a real tax authority.** The tests in this
repository prove the product's rules against the fake gateway and against local stand-ins written from the gateways' public
documentation ([ADR 0075](../adr/0075-fiscal-gateway-port.md), [0079](../adr/0079-nfeio-adapter.md),
[0081](../adr/0081-nfse-documents.md), [0082](../adr/0082-nfce-and-point-of-sale.md),
[0083](../adr/0083-received-invoices-and-the-fiscal-webhook.md)). Every statement about what a gateway answers is
**unverified** until this plan has been run and its results written down.

**No company may issue in production before this plan has passed in homologation**, for each kind of document it will issue and
for the gateway it will use. Homologation documents have no fiscal value (the Documentos sheet says so), but they need the same
real CNPJ and A1 certificate as production, so only the operator can run this.

## 1. What is needed (none of it is in this repository)

| Need | Why |
|------|-----|
| A real CNPJ with a state registration (IE), and a municipal registration for services | Even homologation is issued under a real company |
| The company's **A1 certificate**, uploaded **in the gateway's panel** | It never enters the product ([ADR 0073](../adr/0073-fiscal-module-plan.md)); only its expiry day is typed in the fiscal settings |
| An account at the gateway (Focus NFe or NFE.io) with the company registered, a token per environment | `FISCAL_GATEWAY=focus` or `nfeio` |
| For the **NFC-e**: the CSC and its id, registered where the gateway says | Focus NFe answers `empresa_nao_configurada` without it; where it is set is not documented |
| For the **NFS-e**: a municipality that joined the national standard, the company's IBGE code, the national code of each service | Written by the company and its accountant in the nature's ISSQN terms |
| For **received invoices**: the CNPJ enabled to receive NF-e at the gateway | Unverified that nothing else is needed |
| A person who is an **accountant** to read the results | The product decides no tax fact ([accountant's checklist](accountant-checklist.md)) |

Only the **Simples Nacional** issues (the other regimes are refused, [ADR 0084](../adr/0084-tax-calculation-behind-a-port.md)), customers
with a CPF for the NF-e, and the NFS-e through **Focus NFe only** (NFE.io answers `Unsupported` for the NFS-e, the NFC-e and received
invoices).

## 2. Setting it up

```bash
export FISCAL_GATEWAY=focus
export FOCUS_NFE_TOKEN_HOMOLOGATION=...      # the homologation token
export FOCUS_NFE_ISSUER_CNPJ=...             # the only CNPJ this installation issues for
# optional: FOCUS_NFE_TOKEN_PRODUCTION, FOCUS_NFE_API_URL, FISCAL_POLL_INTERVAL
# for the webhook (section 8):
export FISCAL_WEBHOOK_SECRET=...             # 32 characters or more
export FISCAL_WEBHOOK_HEADER=x-fiscal-webhook-token
```

For NFE.io: `FISCAL_GATEWAY=nfeio`, `NFEIO_API_KEY`, `NFEIO_COMPANY_ID_HOMOLOGATION`, `NFEIO_ISSUER_CNPJ`; production issues stay
refused until the sandbox test of section 3 passed and `NFEIO_SEARCH_CONFIRMED=true` is set.

In the app, fill the **fiscal settings** with the environment `homologation`, the certificate's expiry day, the series, the
municipality's IBGE code, the operation natures (CFOP, CSOSN/CST, PIS, COFINS, and the ISSQN terms of each service) **as the accountant
says**, and register a customer with a CPF and an address in the issuer's state, a product with an NCM and a service.

## 3. The ignored sandbox tests (run by hand)

```bash
FOCUS_NFE_SANDBOX_TOKEN=... FOCUS_NFE_SANDBOX_CNPJ=... \
  cargo test -p conta-verde-infrastructure --lib -- --ignored the_sandbox_authorizes_an_nfe
NFEIO_SANDBOX_API_KEY=... NFEIO_SANDBOX_COMPANY_ID=... NFEIO_SANDBOX_CNPJ=... \
  cargo test -p conta-verde-infrastructure --lib -- --ignored the_sandbox_finds_what_it_made_so_a_second_submit_makes_none
```

Record the result. A failure is information: **write down what the gateway actually answered**, then correct the adapter and the
ADR that listed it as unverified.

## 4. The NF-e (products)

Issue from a completed sale to a customer with a CPF, in the issuer's state, then:

1. **Issue** with every choice made by the person. Expect `processing`, then `authorized` after **Atualizar situação** or the poll;
   the XML and the DANFE download; the access key is 44 digits and names the company's CNPJ, the series and the number.
2. **Asking twice** gives the same document and sends nothing twice.
3. **A rejection**: a product with no NCM is refused before sending; send a body the authority refuses (an invalid NCM, a CFOP that does not fit) and
   read the refusal's words on the sheet; fix it and **Reenviar** under the same id. Note whether the gateway accepts the same reference again.
4. **Cancel** with a reason of 15 or more characters; read the gateway's deadline and the refusal when it has passed; the cancellation XML downloads.
5. **Correction letter** (CC-e): one accepted, one refused (a letter that changes a value), the 20-letter limit not tried.
6. **Invalidation** of a range nobody holds; a range that includes an issued number is refused by the product.
7. **The poll**: stop the app between the issue and the answer, start it again, see the document settle by itself.
8. **A closed month** (accounting > period lock): issuing, cancelling and the late authorization behave as ADR 0073 says.
9. Contingency is shown, never chosen by the product: note what the gateway reports when the authority is out, if it can be provoked.

## 5. The NFC-e (the counter)

1. At **Caixa**: a sale of products paid in one form, the consumer not named; then named (a customer with a CPF). Expect an answer at
   once (`authorized`) and the key's model to be 65.
2. A payment that adds up exactly; **change is not supported**: confirm with the gateway how it is declared before enabling any.
3. Presence choices: the counter's four (in person, delivery, outside the establishment, other). **Confirm with the authority's rules that
   the three the product offers are the right ones** (unverified).
4. **Cancel** an NFC-e; a refusal after the deadline.
5. Without the CSC configured: note the exact error and how the page shows it.
6. The DANFCe is an HTML page the product does not keep: decide with the accountant whether that is enough.

## 6. The NFS-e (services)

1. The nature's national code, taxation and rate written; the issuer's IBGE code set; a municipality **inside** the national standard.
2. **Issue** from a sale with service lines. Read which fields the gateway requires beyond the ones the product sends (the reform's fields,
   the tomador's municipality code, the tax regime) and **write each one down**: the product sends none it was not told is required.
3. **DPS numbering**: the number the company gives is series 1 to 49999 and the next is the biggest plus one. Confirm the authority accepts
   it, and **what it does with a rejected DPS number sent again** (the product keeps it and sends again under the same id: [ADR 0081](../adr/0081-nfse-documents.md)).
4. **A refusal** with the gateway's words, fixed and sent again; **a duplicate reference** answered by asking.
5. **Cancel** with a reason; the gateway's deadline and the municipality's rules for it.
6. The printed form (DANFSe) may be hosted by the authority, not the gateway: the product then keeps only the XML. Decide if that is enough.
7. The package for the accountant carries the NFS-e under its key or its number.

## 7. Received invoices and the recipient's manifestation

1. Have a supplier (or a second homologation company) issue an NF-e **to** the company, then **Notas recebidas > Buscar notas**.
2. Confirm the list's fields, the paging by version, the situation, and what an invoice already declared elsewhere looks like.
3. Declare **awareness**, then **confirmation**; a second word after the final one is refused; **operation not performed** with and without a reason.
4. Declare the same word twice on purpose and read the authority's refusal (the product never repeats a word on its own).

## 8. The webhook

Register the hook at the gateway (`POST /v2/hooks` or its panel) for the events of the documents and of received invoices, with the
`authorization_header` and `authorization` equal to `FISCAL_WEBHOOK_HEADER` and `FISCAL_WEBHOOK_SECRET`, pointing at
`https://<host>/public/fiscal/<company id>/webhook`.

1. A wrong or missing secret gets `401`; the right one gets `202` at once.
2. Issue a document that waits and see it settle without pressing anything; have a received invoice issued and see it listed.
3. **Read the payload the gateway really sends** and write it down: the product reads none of it, but the retry policy, the answer it
   expects and whether it sends one hook per document are unknown, and a burst of hooks must not make the server fall behind.

## 9. NFE.io (only if it will be used)

Only the NF-e is built for it. Run the sandbox test of section 3, check that the search by our reference finds what a first submit made
(so a second submit makes none), and only then set `NFEIO_SEARCH_CONFIRMED=true`. The NFS-e, the NFC-e and the received invoices are
**not built** for it: do not choose it for a company that needs them.

## 10. What to write down, and when it is finished

For each point above: date, who ran it, gateway and environment, the document's id and access key (or DPS), **what the gateway
answered in its own words**, and whether it matched what the ADR assumed. Every mismatch becomes a fix and an update of the ADR that
listed it as unverified. A kind of document may be issued in **production** only when:

- every point of its section passed in homologation, with the results written down;
- the accountant has read them and the [checklist](accountant-checklist.md)'s points that apply to it are decided;
- the A1 certificate's expiry day is set and a reminder exists to renew it.

The regimes other than the Simples Nacional stay refused whatever this plan finds.
