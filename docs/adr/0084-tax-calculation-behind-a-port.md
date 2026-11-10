---
title: "ADR 0084: The tax calculation behind a port, and the accountant's checklist"
status: accepted
date: 2026-11-10
tags: [adr, fiscal, phase-7, tax]
---

# ADR 0084: The tax calculation behind a port, and the accountant's checklist

## Context

[ADR 0073](0073-fiscal-module-plan.md) leaves the tax calculation of the regimes other than the Simples Nacional to the last slice and
says it must be reviewed by an accountant before the release; the NF-e, NFC-e and NFS-e refuse those regimes today ("the calculation is
not built"). **The product embeds no tax law**, and nothing in this repository can decide what is right for a company's taxes.

## Decision

### A pure calculation from the company's own rules, behind a port

`domain::item_tax` and `document_tax` work out ICMS, PIS and COFINS of an item from the **rule the company wrote**: the base is the item's
value less its discount (cents; a discount above the value or an amount in fractions of a cent is refused, never rounded), each amount is
`base x rate / 100` rounded to the cent, **halves away from zero** (the product's one rounding rule), a tax the rule gives **no rate** for is
not calculated (`None`, never a guess) and a rate of 0 is an amount of 0. The invoice's total of a tax is the **sum of the items' rounded
amounts**, never the rounding of the sum, so the total is what the items print. `application::TaxCalculator` is the port (pure: same
input, same output, exact decimals, no I/O), and `RuleRateCalculator` the implementation: **the place a certified engine, or a
calculation an accountant signed, would be plugged in**. Nothing decides a rate, a base reduction, a regime, ICMS-ST, DIFAL or a benefit.

### Tested against an independent reference

The domain tests hold the calculation to an **integer reference in whole cents and hundredths of a percent** (no decimal type) over more than
five thousand combinations of values, discounts and rates, plus the cases that are easy to get wrong: the half of a cent, a base of 0, 100
per cent, a total that differs if the sum is rounded instead of the items. A generic `a_calculator_keeps_its_promises` states what any
implementation of the port must hold to.

### It is **not connected to issuing**

No issue uses the calculator and **the regimes other than the Simples Nacional stay refused** for the NF-e, the NFC-e and the NFS-e (a
service-level contract, run on both storage adapters, holds that: the refusal names the Simples Nacional, nothing is sent and no document is
written). Connecting it means deciding what is sent to the gateway and with which fields, which is the accountant's point 2 of the
checklist. **The roadmap item "Tax calculation review by an accountant" stays unticked** until a person who is an accountant fills in the
sign-off of [`docs/fiscal/accountant-checklist.md`](../fiscal/accountant-checklist.md); software never ticks it.

## Consequences

- There is a tested place to put the calculation, and a checklist that says exactly what the code assumes (twelve points) so that a
  review is possible.
- The company still cannot issue under the Lucro Presumido, and the settings still accept it so that it can be set up.
- **What cannot be validated here, and needs the accountant:** that the base, the rounding and the flat per-rule rates are right for any
  real company; everything under points 4, 5, 11 and 12 of the checklist is not modelled.

## What this does not decide

The calculation of ICMS-ST, DIFAL, IPI, retentions or the cumulative regime of PIS/COFINS; what is sent to the gateway; which regimes
are ever supported.
