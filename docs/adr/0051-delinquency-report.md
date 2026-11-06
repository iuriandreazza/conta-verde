---
title: "ADR 0051: The delinquency report"
status: accepted
date: 2026-11-06
tags: [adr, collection, reports, phase-4]
---

# ADR 0051: The delinquency report

## Context

The overdue list of [ADR 0035](0035-entry-reports.md) answers "which installments are late", oldest first and cut
at a limit. Collection asks another question: **who** owes, how much, and **how late**. This is the last item of
Phase 4, read-only and made of what already exists.

## Decision

`GET /companies/{id}/delinquency?on=YYYY-MM-DD` (segment `delinquency`, `finance` area, read only) answers, for the
day `on` (required: the server reads no clock), what is **late on that day**: the installments of **receivables that
are not cancelled** that fell due **before** `on` and still have something left (the amount less what the
settlements that are not reversed settled, ADR 0030), grouped by customer.

- **Buckets** by how many days late each installment is (`on` less its due day): **1 to 30**, **31 to 60**,
  **61 to 90** and **over 90**. The report has the owed amount and the number of installments of each, and of all.
- **Customers**, each with what it owes, the number of late installments, the oldest due day, and its owed amount in
  each bucket; **the biggest debt first**, then by party id. Installments of an entry with no customer are a row of
  their own with no party (`party_id` is `null`), after the ones that have a customer when the debts tie.
- Amounts are exact decimal strings, summed in `domain` (`DelinquencyReport::build`, a function of the day and the
  late installments, tested on worked cases); nothing is stored.
- The installments come from the entries' port (`receivable_installments_due`, which the reminders already use, over
  every day up to the one before `on`), so both adapters answer alike and the voided and the settled are excluded
  once, in one place. There is no limit: a business's late installments are a short list; a limit would make the
  totals lie.

The web app has an **Inadimplência** page under Financeiro: a day (today by default), a tile per bucket, and a
table by customer (names from the parties register). Reminders sent and open charges per customer are not in the
report: the per-entry panel (ADR 0045, 0048) is where they are read.

## Consequences

- The report is as late as the settlements: a payment that a provider has made but whose webhook has not arrived
  (ADR 0046) still shows as owed.
- Interest and fines that were not yet charged are not in the owed amount: it is what the installments say.

## Revisit when

A company has tens of thousands of late installments (a query that sums in SQL), the report needs to be exported
(ADR 0013), or it must say when each customer was last reminded.
