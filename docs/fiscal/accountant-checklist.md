---
title: "Accountant's checklist for the regular regimes' tax calculation"
status: draft
date: 2026-11-10
tags: [fiscal, accountant, checklist, tax-calculation]
---

# Accountant's checklist: the tax calculation of the regular regimes

**This is a request for a human review, not a statement that anything is correct.** The product embeds no tax law
([ADR 0073](../adr/0073-fiscal-module-plan.md), [ADR 0084](../adr/0084-tax-calculation-behind-a-port.md)). It has a calculation that
works from the rates the company writes in its own tax rules, and **no invoice uses it**: a company under any regime other than the
Simples Nacional cannot issue until a person who is an accountant has gone through this list, decided each point for the companies the
product is meant for, and signed. Software never ticks the roadmap item, and never marks this document as signed.

Nothing here is advice. Each line says what the code does today so that it can be confirmed, changed or refused.

## What the code does today

| # | Point | What the code does | Decision (accountant) |
|---|-------|--------------------|-----------------------|
| 1 | **Base of each tax** | The base is the value of the item **less its unconditional discount**, in cents. Freight, insurance, other charges, IPI and ICMS-ST are not part of it and are not calculated. | |
| 2 | **Which taxes** | ICMS, PIS and COFINS only, each from a rate the company types in the rule of its operation nature (0 to 100, two decimals). A tax with no rate is **not calculated** (no amount), a rate of 0 is an amount of 0. | |
| 3 | **Rounding** | Each item's tax is `base x rate / 100` rounded **to the cent, halves away from zero**. The invoice's total of a tax is the **sum of the items' rounded amounts**, never the rounding of the sum. | |
| 4 | **ICMS reduction of the base, deferral, exemption, ST, DIFAL, FCP, benefit codes** | Not modelled. No field, no rule. | |
| 5 | **PIS/COFINS regime** (cumulative or not-cumulative, credits, retentions) | Not modelled: a flat rate per rule. | |
| 6 | **CST / CSOSN per NCM** | The company types the code in the rule; the product only checks its **shape** and picks the rule by regime and the longest NCM prefix. | |
| 7 | **CFOP** | The company types one for inside the state and one for another, per nature. The product picks by the customer's state. | |
| 8 | **ISSQN of a service** | The national code, the taxation and the rate come from the nature; nothing is calculated for the NFS-e (the rate is sent when the company set it, otherwise the municipality's system supplies it). | |
| 9 | **NFC-e** | Inside the state, no change given, same calculation as the NF-e. | |
| 10 | **Which regimes and which companies** | Simples Nacional issues (classification only, nothing calculated). Lucro Presumido is accepted in the settings and **refused when issuing**. MEI, Simples with excess and Lucro Real are refused in the settings. | |
| 11 | **Totals printed on the invoice** | The gateway computes and prints the invoice's own totals from what it is sent; the product sends no tax amounts today. | |
| 12 | **Retentions, withholdings and the like** | Not modelled. | |

## What must be decided before the regular regimes can issue

1. Which of the points above are acceptable as they are, which need a field or a rule, and which make the regime **not supported** at all.
2. What the calculation sends to the gateway (the amounts per item and per invoice) and which gateway fields carry them (Focus NFe and
   NFE.io differ and neither was exercised).
3. A set of **worked examples** (a table of items, rules and the expected amounts to the cent) that the product's tests will run against
   (`crates/domain/src/tax_calculation.rs` already checks the rounding against an independent integer reference; the expected values of
   real cases are the accountant's to give).
4. Whether the product may calculate at all for the companies it is aimed at, or must always take the amounts from the gateway or the
   accounting system.

## Sign-off

| | |
|---|---|
| Name | |
| Professional registration (CRC) | |
| Date | |
| Scope of what was reviewed (points and regimes) | |
| Decision | |
| Conditions | |

Until this table is filled in by a person, the roadmap item "Tax calculation review by an accountant before the release" stays
unticked and the regimes other than the Simples Nacional stay refused.
