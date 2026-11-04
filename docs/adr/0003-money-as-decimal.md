---
title: "ADR 0003: Money is an exact decimal"
status: accepted
date: 2026-11-04
tags: [adr, finance]
---

# ADR 0003: Money is an exact decimal

## Decision

Monetary amounts are never `f32` or `f64`. In Rust they use `rust_decimal::Decimal`, stored in PostgreSQL as `NUMERIC(19, 4)`,
and the API serializes them as strings. Rounding is explicit and follows the rule of each document (for example, installments
split with the remainder assigned to the last one, so the parts always sum to the total).

The dependency is introduced together with the first feature that handles money (Phase 1).

## Consequences

Slightly more verbose arithmetic, in exchange for no accumulated rounding errors in ledgers and reports.
