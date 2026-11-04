---
title: "ADR 0003: Money is an exact decimal"
status: accepted
date: 2026-11-04
tags: [adr, finance]
---

# ADR 0003: Money is an exact decimal

## Context

Ledgers, cash flow, average cost and tax bases must sum exactly; a one-cent drift across
thousands of installments is a support ticket and a legal problem on fiscal documents.
Binary floating point cannot represent 0.10 exactly. Brazilian tax rules also prescribe
rounding at specific steps (per item, per document) with more than two decimals in unit
prices and quantities.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| `f64` | Simple, fast | Accumulated rounding error; unacceptable |
| Integer cents (`i64`) | Exact, fast | Unit prices and tax rates need more than two decimals; every multiplication needs manual scaling |
| `rust_decimal::Decimal` + `NUMERIC` (chosen) | Exact, 28 significant digits, explicit rounding, maps directly to PostgreSQL | Slightly more verbose arithmetic |
| Arbitrary precision (`bigdecimal`) | Unbounded | Slower, no need for unbounded precision |

## Decision

Monetary amounts are never `f32` or `f64`. In Rust they use `rust_decimal::Decimal`,
stored in PostgreSQL as `NUMERIC(19, 4)` for amounts and `NUMERIC(19, 6)` for unit prices,
quantities and rates. The API serializes them as strings (`"1234.56"`), never as JSON
numbers, so clients cannot lose precision by accident.

Rounding is explicit and follows the rule of each document:

- Installments: split with the remainder assigned to the last installment, so the parts
  always sum to the total.
- Sales and purchases: item totals rounded to 2 decimals, then summed; document taxes
  computed on the rounded bases.
- Average cost: kept at 6 decimals, rounded to 4 for storage.

The currency is BRL only. A `Money` value object with an explicit currency field is not
introduced until a second currency is needed.

The `rust_decimal` dependency enters `domain` with the first feature that handles money
(Phase 2).

## Consequences

- Slightly more verbose arithmetic in exchange for no accumulated rounding errors.
- Every rounding point is a named function in `domain` with a unit test against a worked
  example, so fiscal reviewers can check the rule.
- Clients (SPA, integrations) must parse decimal strings; the OpenAPI document declares
  the format.

## Revisit when

Multi-currency is needed (a `Money { amount, currency }` type), or a fiscal rule requires
a rounding mode `rust_decimal` does not provide.
