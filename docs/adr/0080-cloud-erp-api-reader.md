---
title: "ADR 0080: Reading a company's books from the cloud ERP's API"
status: accepted
date: 2026-11-10
tags: [adr, import, migration]
---

# ADR 0080: Reading a company's books from the cloud ERP's API

## Context

[ADR 0064](0064-entry-import-from-a-file.md) imports entries from a CSV that a script converts from the spreadsheets the
old system exports. Spreadsheets lose information (one row per category and cost center piece, no stable ids, no payment
detail) and an import made from them had to be redone. The old system has a public REST API (OAuth 2.0,
`api-v2.contaazul.com`, described by the vendor's published OpenAPI files) that returns the same registers with their own
ids and, per installment, its split and its payments.

## Decision

Two scripts outside the application (standard library only, like the converter they replace), so the layers are untouched:

1. **`scripts/erp-api-reader.py`** reads the books into local JSON files, as the API answered.
   - `login` runs the authorization code flow (the app's registered address, from `ERP_OAUTH_URL`; the code is pasted back
     with `--callback` when the redirect is not a loopback address) and keeps the tokens in
     `~/.config/conta-verde/erp-api-token.json` (mode 0600, outside the repository). The client id and secret come from
     the environment or an `--env-file`, never from an argument.
   - `fetch` reads categories (with their parents), cost centers, financial accounts, people (every kind and profile: the
     listing defaults to individuals who are customers), products (active and inactive), receivables and payables
     (windows of 90 days, the listing's own limit), transfers and opening balances (windows of one year, the endpoints'
     limit), then **the detail of every installment**, which alone carries the split by category and cost center and the
     payments. A 401 renews the token once; a 429, a 5xx or a dropped connection waits and tries again; `--reuse` finishes a
     run that failed halfway.
2. **`scripts/erp-to-conta-verde.py`** turns the files into what conta-verde takes and loads it.
   - `plan` writes `registers.json` (categories, cost centers, bank accounts, transfers, opening balances), `parties.csv`
     (ADR 0025), `entries.csv` (ADR 0064) and `report.txt`, which says everything that did not carry over as it was.
     Money is whole cents; the entries add up, per kind, to the cent of the source, for the amount and for what was paid.
   - `load` creates the registers in a running API (everything active, because the importers reach only active records;
     what was inactive is switched off at the end), imports the two files in pieces (every piece is checked before the
     first is written), then the transfers and opening balances. The parties go once, into a company that has none: a party
     without a document has nothing to tell it from itself.

What the mapping decides, and the report names:

- **Groups.** conta-verde fixes the DRE and cash-flow groups on a root; the old system puts them on the leaves. A root takes
  the group most of its descendants have; a root the old system leaves ungrouped is chosen with `--groups`.
- **One row per piece.** An installment split by category and cost center becomes one row per piece, the amount and the
  payment shared in cents with the remainder on the last; the split lives on the event, so it is scaled to the installment.
- **Payments.** An installment paid more than once is joined into one settlement on the last day; a fee taken from the
  cash is carried as a discount, so the account's balance agrees; a lost installment that was paid in part is imported as
  paid. Lost without a payment and worth-zero installments are left out.
- **Opening balances.** conta-verde counts an account's movements only from its opening day, while the old system counts
  them all, so an opening that came after the first movement is moved back to it.
- **Parties.** A document that fails its check digits, repeats on another party, or an address or e-mail that is not
  valid is left blank; the party stays.

The importer of ADR 0064 named a category by its **parent's name** (`Parent > Child`), not by the whole path its description
promised; a category three levels deep could not be found. It now names the whole path from the root, as the exports do.

## Consequences

- No new endpoint, migration or dependency; one fix in the entries importer's lookup, covered by the contract test.
- Tests use a local stand-in for the API and small made-up registers, never the real ones.
- The result is checked against the old system's own balance of every account (`saldo-atual` against `GET /balances`),
  which is how the opening and fee rules above were found.
- The vendor's name appears only in the API addresses the tool must call, never in the UI or the docs.

## Revisit when

- Imports from this source are routine, which would justify a connector inside the application (a port with a job,
  [ADR 0059](0059-job-queue.md)) instead of scripts.
- The import must be repeated over a live company: the entries are idempotent by reference, the parties are not.
