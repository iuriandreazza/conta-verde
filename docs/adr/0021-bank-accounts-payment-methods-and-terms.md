---
title: "ADR 0021: Bank accounts, payment methods and payment terms"
status: accepted
date: 2026-11-05
tags: [adr, registers, finance]
---

# ADR 0021: Bank accounts, payment methods and payment terms

## Context

Entries are settled in an account, by a method, and receivables are generated from payment terms
(Phase 2). The registers are needed first and must be shaped so that those features need no
migration of their data. Three decisions are easy to get wrong and expensive to undo: what
makes one account "the same" as another, what a payment method's identity is, and how a term
splits an amount.

## Decision

### Bank accounts

- **Kinds:** `checking`, `savings`, `cash`, `card`. The kind is fixed at registration, since what an
  account is asked for and what Phase 2 does with it depend on it. A cash account takes no bank,
  branch or number, and a card takes its issuing bank only: sending the rest is refused, not
  ignored. A card number in particular is never stored, since the audit trail is append-only and
  would keep it for good. Closing and due days of a card are not
  here; they are an `ADD COLUMN` when the card bills of Phase 2 need them.
- **No balance.** Opening balances, transfers and balances per account are Phase 2, with the
  money type and the rounding rules they need ([ADR 0003](0003-money-as-decimal.md)). The register
  holds no money.
- **Bank code:** 3 digits, the code the Banco Central gives participants of its payment system
  (the old COMPE code), or 8 digits, the ISPB, for an institution that has none. It is optional.
  The list is published as open data by the Banco Central (the participants of the STR); it is not
  copied here, so the code is checked for its shape and not for its existence.
- **Branch and account number** are stored in one form: upper case letters and digits, without
  spaces, dots or hyphens, the check digit kept (`1234-5` is `12345`), at most 10 and 20
  characters. One form is what lets a unique index see "the same account" and, in Phase 2, lets an
  OFX file be matched to an account; whether the identifiers of real OFX files match this form has
  to be verified against real files then.
- **Uniqueness:** the name, per company, ignoring case; and the kind, bank, branch and number together
  when the last three are known (the kind is part of it because some banks give a savings account the
  branch and number of the checking account). An account with an incomplete identity never clashes on
  it. The index sees the same text twice, not the same account written differently: the ISPB instead
  of the code, a branch with leading zeros or a number without its check digit are other values.

### Payment methods

- A name and a **kind** from a closed set: `cash`, `pix`, `bank_slip`, `credit_card`,
  `debit_card`, `bank_transfer`, `check`, `other`. The keys are stored and do not change; the
  labels live in the web app. The kind may be changed, since it classifies and does not identify.
- The codes a fiscal document asks for (the NF-e's payment form) come from the official table and
  belong to the fiscal phase ([ADR 0006](0006-fiscal-last.md)); they are not mapped here.
- Fees, the account a method settles into and provider links are Phase 4
  ([ADR 0010](0010-payment-providers.md)).
- Unique by name per company, ignoring case. There is no default list: the empty page says what
  to add.

### Payment terms

A name and an ordered list of installments, each a number of **days** after the date the term is
applied to and a **percentage** of the amount ("30/60/90", "50% now, 50% in 30 days").

- **Days only.** "The same day of next month" is a different rule (it depends on the calendar) and
  is left for when it is needed. Day 0 is "à vista".
- **Rules of the term**, checked by the domain, each with its own error: at least one and at most 24
  installments; days from 0 to 3650, strictly increasing; every percentage more than 0 and at most
  100, with at most **two decimals**; and the percentages add up to **exactly 100**.
- **No rounding.** A percentage with more than two decimals is refused, never rounded. Splitting an
  amount into shares that do not come out even is the rule of whoever applies the term (the
  remainder goes to the last installment, [ADR 0003](0003-money-as-decimal.md)); the web form
  applies the same rule to fill in "33.33, 33.33, 33.34" for three equal installments, and the
  domain only checks that the sum is whole. Percentages travel as strings and are kept without
  trailing zeros (`50.00` is `50`), so every adapter reads the same.
- **Storage:** a child table keyed by (term, position) whose foreign key carries the company, so an
  installment cannot be in another company than its term. The installments are replaced as a whole
  inside the transaction that writes the term and its audit entry, so a refused edit (a duplicate
  name) leaves the old installments in place. The database refuses what a row can show (position
  1 to 24, days 0 to 3650, percent above 0 and at most 100, the same day twice); **the sum and the
  order are the domain's alone**, so a write that bypasses the domain could store a term that does
  not add up.
- **Audit:** the installments are one field in the trail, in a canonical text (`0:50,30:50`), so an
  edit that changes nothing compares equal and one that changes them records the before and after.
- Applying a term to an amount and a date, and splitting the amount, is the entries' job (Phase 2).

## Consequences

- Both registers copy the shape of the earlier ones: a domain type that validates, a port with a
  contract test run on Postgres and in memory, an audited full replacement, lists as
  [ADR 0018](0018-list-endpoints-search-filter-sort.md) says.
- What a single row can show is enforced by the database too (shapes, kinds, cash without a bank,
  the two unique indexes); the tests name the constraint that refuses each bad row.
- Concurrent edits of one record are last write wins, as for products.

## Revisit when

An account needs a currency, a card needs its billing cycle, or a bank code has to be checked
against the Banco Central's list (it would be a table loaded from their open data, not a constant).
