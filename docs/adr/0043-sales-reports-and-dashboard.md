---
title: "ADR 0043: Sales reports and the dashboard"
status: accepted
date: 2026-11-06
tags: [adr, sales, reports, dashboard, phase-3]
---

# ADR 0043: Sales reports and the dashboard

## Context

Phase 3 ends with two things a person reads rather than writes: reports of what was sold (by customer, product, seller and period, and
the ABC curve) and a dashboard whose figures are real instead of the sample data of [ADR 0020](0020-app-shell-charts-and-sample-data.md).
[ADR 0037](0037-sales-module-plan.md) schedules both as the last slice. They are delivered in two pull requests, the reports first.

## Decision

### Sales reports

`GET /companies/{id}/sales-reports?by=customer|product|seller&from=&to=` (`sales` area, read only; both dates are required because the
server reads no clock; an unknown `by`, a bad date or `from` after `to` is a `400`) answers with the **completed** sales sold in the span,
**net of returns**, grouped and ranked: biggest first, the sales with no seller last, the id breaking a tie. An order and a cancelled sale
count for nothing. Each row has the customer, product or seller (null for the sales that name no seller), how many sales it is in, the value
sold, its **share** of the total and the **cumulative share**, in percent with at most two decimals (rounded half away from zero), and an
**ABC class**.

Like the commission statement (ADR 0041), the report is **composed in the use case** from the sales' port (`list_by_company` with
`sold_between`, and `returns_of_sales`) and added up when read, never stored and never as SQL in two adapters: the refund of a return is
cumulatively rounded line by line, which SQL would have to repeat. The per-line net of returns is **one domain function**
(`net_of_lines`) that the commissions and the reports both call, so the base of a commission and the value in a report cannot disagree.
More than 2000 sales in a span are refused (`400`): narrow it. The sales are read by the same helper as the statement.

By product, a sale counts once for each product in it and its value is the net of that product's lines; by customer or seller it is the net of
all its lines. A product returned in full is listed with 0 and a class of C.

### The ABC curve

A row's class is given by the cumulative share **before** it: A while that is under 80%, B while it is under 95%, C from there. So the
row that crosses 80% is still an A and the one that crosses 95% still a B, and a row that ends exactly on 80% leaves the next one a B. The
classes are worked out from the exact values and never from the two-decimal shares the answer shows, and when the total is 0 or less every
row is a C with a share of 0. The curve is given for every grouping; it means most by product.

### The dashboard (next pull request)

The dashboard is built on the client from the endpoints that already exist, plus the sales report, with no `/dashboard` endpoint: every role
reads every area but the members', so the page asks the finance and sales routes directly, each card on its own, and a card that fails
shows its own notice and never takes the page down. The client sends today's date (the server reads no clock) and stamps the page with
the moment it was loaded ("atualizado em"). It replaces `sampleData.ts` and its test, which are deleted, and the 3/6/12 months control by the shared
`PeriodFilter`. What it shows, from the review of the screens (S4 to S7): the balance of each account and the total (`/balances`), with the pending
statement lines to reconcile and a link to Conciliação (`statement-lines?status=pending`, shown as "N+" when there is more than a page);
receivable and payable as two mirrored blocks, each with what is overdue, what is due today and what is due next (from `/reports/overdue` and
the daily forecast of `/cash-flow`), each with a link to create one; the monthly flow (`/cash-flow` by month); and the sales of the month with
the top products and customers (`/sales-reports`).

## Consequences

- A report changes with the returns made after it was first read: it says what a sale is worth now.
- A span with more than 2000 sales has to be cut by the person; there is no paging of a report.
- Names are the web's to resolve (the registers are read once for the page), so a company with more than 5000 customers or products shows "—" for the ones past that.
