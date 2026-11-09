---
title: "ADR 0074: Fiscal settings per company"
status: accepted
date: 2026-11-09
tags: [adr, fiscal, phase-7]
---

# ADR 0074: Fiscal settings per company

## Context

[ADR 0073](0073-fiscal-module-plan.md) decided what the fiscal phase shares: a `fiscal` area, a gateway behind a port, the
certificate left at the gateway, no tax law embedded in the code, and the gateway owning the document numbers. This slice is
the data a document is made from, before any document exists: who the issuer is, which regime it is in, which environment its
documents go to, which series it uses, when its certificate expires and what it calls each kind of operation, with the codes
and rates its accountant tells it to put on the items.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Natures and tax rules as two registers of their own (ids, lists, cursors) | Each is edited alone | Two more registers with list endpoints and filters for what is a dozen rows; a document would point at a row that can change |
| **Natures and rules held by the settings, replaced as a whole** (chosen) | One aggregate, one audit trail, one transaction, one page; the same shape as a price list's items or a term's installments | A nature has no id of its own: its `code` is the key |
| The issuer's name and address read from the company or a party | Nothing to type twice | The company has only a name today, and a party's address is a customer's; the issuer's snapshot must be one the fiscal settings own |
| Upload the certificate through the product | The person does one thing | Rejected in ADR 0073: the key and its password would pass through our process |

## Decision

### One record per company

`FiscalSettings` (`domain`) is one per company, replaced as a whole by `PUT /companies/{id}/fiscal/settings`, audited as a
whole with the old and the new value of every field that changed ([ADR 0008](0008-audit-trail.md); the audit id is the
company's). The same settings again write nothing. `GET` answers `null` while the company has not set them; the history is
`GET .../fiscal/settings/history`. The `fiscal` area is the area of [ADR 0073](0073-fiscal-module-plan.md): the owner, the
admin and the `financial` role write, everyone else reads. Nothing here calls a gateway.

- **The issuer**: legal name (1 to 150 characters), **CNPJ** (a CPF is refused; the alphanumeric CNPJ is accepted, stored
  without punctuation), state registration (IE: digits, 2 to 14, written without the punctuation states give it, or `ISENTO`),
  municipal registration (optional, at most 20 characters) and a whole **address** (the same shape as a party's). Only the
  **shape** is checked: each state has its own check digits for the IE and the product does not know them (**verify with an
  accountant**).
- **The regime**: `simples_nacional` or `lucro_presumido`. MEI, Simples with excess of the sublimit and Lucro Real are
  **refused** (`422`), not approximated: what changes for them is not known to the authors.
- **The environment**: `homologation` (the default) or `production`; the gateway's two hosts and tokens follow it
  ([ADR 0073](0073-fiscal-module-plan.md)). **Production needs the day the certificate expires**, so a company cannot be moved
  to it without saying that it has one.
- **The series** of each kind (NF-e, NFC-e, NFS-e): a number from 1 to 999 (**verify with an accountant** which ranges the
  SEFAZ reserves). They are what the person expects and what an invalidation needs; **the number of a document is not here**:
  the gateway assigns it and the document records what came back.
- **The certificate**: only `certificate_valid_until`, a day the person types from the certificate. The certificate and its
  password are never sent, kept or asked for by the product, and the page says so. A warning 30 days before it and the refusal
  to issue after it are in slice 4 (a comparison of this day with the moment the caller passes in).

### Operation natures and their tax rules

A company registers its **operation natures**, up to 50: a `code` (the stable key a fiscal document will record: 1 to 30 lower
case letters, digits, `-` or `_`; typed in any case it is stored lower case), a `name` (1 to 60 characters; code and name are
each unique within the company, the name ignoring case), a **kind**, an `active` flag and the rules.

- A **product** nature carries a **CFOP for a sale inside the state** (four digits, the first `5`) and one **for another state**
  (four digits, the first `6`), and up to 20 **tax rules**. That the first digit says where the goods go is the CFOP's own
  structure; which CFOP suits an operation is the company's and its accountant's.
- A **service** nature has **no CFOP and no rules**: the tax of a service is municipal and is decided with the NFS-e
  (slice 5, which adds its fields to this record in its own ADR).
- A **tax rule** is for one **regime** and, optionally, an **NCM prefix** (2 to 8 digits: the products whose NCM begins with it;
  no prefix is the nature's general rule; at most one rule per regime and prefix). It holds the **ICMS code** (a CSOSN of three
  digits for the Simples Nacional, a CST of two for Lucro Presumido), the **PIS** and **COFINS** CSTs (two digits) and, all
  optional, the three **rates** (0 to 100 with at most two decimals, held as exact decimals, never floats). Rules for both
  regimes may exist side by side, so changing the company's regime changes which ones apply and deletes none.

**What the product does not know, on purpose.** There is no table of CFOPs, CSOSNs, CSTs or rates in the code, no suggestion
of which to use and no check that a code is right for the regime beyond its length. The company and its accountant fill them
in. **The consequence is that a wrong code is the company's, and the checklist of slice 8 lists every code the tests use as an
example, not as advice.**

### How it is stored

`fiscal_settings` (one row, `company_id` as the primary key), `fiscal_natures` and `fiscal_tax_rules` (children by position,
replaced in the same transaction as the settings and their audit entry), with a `CHECK` for every rule one row can show:
the shapes, the ranges, the pairing of a nature's kind with its CFOPs, the ICMS code by regime, the rate's two decimals, and
**production needs the certificate's day**. What spans rows (a code and a name used once, a rule once per regime and prefix)
is a unique index and, in the domain, the number of natures and rules; the migration's comments say which is which. The
Postgres adapter reads the three tables in one snapshot and rebuilds the record through the domain, so a row that no longer
satisfies a rule is a storage error, never a silent value. The in-memory adapter holds the same record; `contract.rs` runs
the same test on both (`stores_the_fiscal_settings`, `fiscal_settings_are_replaced_and_audited`) and the Postgres adapter has
a test that the database refuses what the domain would.

### The page

A **Configurações fiscais** page under a **Fiscal** group of the two-level menu, gated on `useCanWrite("fiscal")`: the issuer,
the regime and environment, the series, the certificate's day with its status (a warning when it expires in 30 days or fewer,
an error once it has), and the natures in a table whose rows open in a sheet with their rules. Everything is saved by one
button, since the settings are replaced as a whole. There is no file field and no password field anywhere.

## Consequences

- A company can say how it issues before anything can be issued; the API and the page refuse what the SEFAZ's own shapes would.
- A document of slice 4 will copy what it needs from these settings (a snapshot), so a later change never reinterprets one.
- Wrong codes are possible and are the company's to get right; the product cannot tell.
- Tests never depend on a real tax rule: the codes in them are examples.

## Revisit when

A company needs more than 50 natures or a rule narrower than an NCM prefix (a CEST, an origin of the product); a nature must be
reordered or referenced by something that needs an id; the certificate's day should be read from the gateway instead of typed
([ADR 0073](0073-fiscal-module-plan.md)); or the regimes now refused must be issued for.
