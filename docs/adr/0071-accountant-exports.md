---
title: "ADR 0071: Exports for the accountant"
status: accepted
date: 2026-11-09
tags: [adr, accounting, export]
---

# ADR 0071: Exports for the accountant

## Context

[ADR 0067](0067-accountant-and-reporting-plan.md) decided what a month's package holds, its formats and its limits, and
left the **layout of every file** to this ADR: an accountant's software reads these files by position, so the columns do
not change without a new ADR. The exit criterion of the phase is that the accountant closes a month from their own login
and **downloads everything for their accounting software** ([ADR 0069](0069-period-lock.md) closes it).

## Decision

### Routes (`accounting` area, all `GET`, a read for every role that reaches the area)

| Route | Answers |
|-------|---------|
| `/companies/{id}/accounting/exports/entries?from=&to=&format=` | the entries of the span |
| `/companies/{id}/accounting/exports/settlements?from=&to=&format=` | the settlements of the span |
| `/companies/{id}/accounting/exports/transfers?from=&to=&format=` | the transfers of the span |
| `/companies/{id}/accounting/exports/package?from=&to=&format=` | a zip with the three files, the attachments and a list of them |

`from` and `to` are both required (the server reads no clock), `from` not after `to`, at most 366 days (`400`). `format`
is `csv` (the default) or `xlsx`; anything else is `400`. The answer is always `Content-Disposition: attachment`, with
`X-Content-Type-Options: nosniff` and `Cache-Control: no-store`.

### What each file holds

Entries cancelled, settlements reversed and transfers reversed are **left out**: they have no effect on the books and
the audit trail keeps them ([ADR 0008](0008-audit-trail.md)). Amounts are written exactly as stored, never through a
float, and are **never negative**: the direction is a column, so a spreadsheet never sees a number that starts with `-`.
Dates are `YYYY-MM-DD`. Names are the records' current names **including inactive ones**: an entry of last year may point
at a category that was switched off since.

**`entries`**: one row per installment of the entries whose **competence date** is in the span, by competence date, then
entry id, then installment.

| # | Column | Holds |
|---|--------|-------|
| 1 | `entry_id` | the entry's id |
| 2 | `kind` | `receivable` or `payable` |
| 3 | `description` | as typed |
| 4 | `party` | the party's name, empty when none |
| 5 | `party_document` | its CPF or CNPJ without punctuation, empty when none |
| 6 | `category` | the whole path of a subcategory, `Root > Child > Grandchild`; the name for a root |
| 7 | `cost_center` | the name, empty when none |
| 8 | `competence_date` | the entry's |
| 9 | `installment` | this installment's number |
| 10 | `installments` | how many the entry has |
| 11 | `due_date` | this installment's |
| 12 | `amount` | this installment's |

**`settlements`**: one row per settlement **settled in the span** (the cash basis), by `settled_on`, then id.

| # | Column | Holds |
|---|--------|-------|
| 1 | `settlement_id` | the settlement's id |
| 2 | `entry_id` | the entry it pays |
| 3 | `kind` | the entry's: `receivable` or `payable` |
| 4 | `description` | the entry's |
| 5 | `party` | the entry's party's name |
| 6 | `category` | the entry's category, as in `entries` |
| 7 | `cost_center` | the entry's |
| 8 | `installment` | the installment it settles |
| 9 | `settled_on` | the day |
| 10 | `bank_account` | the account's name |
| 11 | `payment_method` | the method's name, empty when none |
| 12 | `applied` | the part of the installment it settles |
| 13 | `interest` | |
| 14 | `fine` | |
| 15 | `discount` | |
| 16 | `cash` | `applied + interest + fine - discount`: what moved in the account |

**`transfers`**: one row per transfer **occurring in the span**, by day, then id.

| # | Column | Holds |
|---|--------|-------|
| 1 | `transfer_id` | |
| 2 | `occurred_on` | |
| 3 | `from_account` | the name |
| 4 | `to_account` | the name |
| 5 | `amount` | |
| 6 | `note` | empty when none |

**The package** is a zip, stored without compression, holding `entries.{csv|xlsx}`, `settlements.{csv|xlsx}`,
`transfers.{csv|xlsx}`, `attachments.csv` and the files attached to the entries of the span, each at
`attachments/{entry_id}/{attachment_id}-{name}`. The name is typed by a person, so `/`, `\`, `:`, `*`, `?`, `"`, `<`,
`>`, `|` and control characters become `_` (it cannot leave its folder, and Windows, where an accountant most likely
unpacks it, can write it), a leading `.` and a trailing `.` or space are dropped, and the attachment's id in front keeps
two files of the same name apart. `attachments.csv` has `attachment_id`, `entry_id`, `file_name`, `content_type`, `size`
and `path`, in the order of the `entries` file (and, within an entry, the order the files were attached), and so are the
files in the zip. An attachment that was removed is not in it. The zip's timestamps are fixed (1980-01-01): the server
reads no clock.

### Formats

- **CSV**: UTF-8 with a byte-order mark, `;` as separator, `.` as decimal point, lines end in `\r\n`, a cell holding `;`,
  `"` or a line break is quoted with `"` doubled. A cell that a spreadsheet would run as a formula (it starts with `=`,
  `+`, `@`, a tab or a carriage return, or with `-` unless it is a plain number) gets a `'` in front, the same rule as the
  web app's CSV (`web/src/lib/csv.ts`).
- **XLSX**: the same columns, five parts (`[Content_Types].xml`, `_rels/.rels`, `xl/workbook.xml`,
  `xl/_rels/workbook.xml.rels`, `xl/worksheets/sheet1.xml`), text as inline strings, **amounts as numeric cells** carrying
  the decimal text as stored (no float is ever made), dates as text. Characters XML 1.0 forbids are dropped. No styles.
- **Zip and XLSX are written by hand** in `api` (`export_files.rs`): a zip writer with a table-driven CRC-32, ZIP64 not needed
  (a package is at most 200 MiB and each file smaller than 4 GiB), UTF-8 names flagged. Tests compare the bytes and read
  the zip back with a small reader written for the test.

### Limits (a cut-off file is never made)

At most **50,000 rows per file**, **500 attached files** and **200 MiB** of attachments: past any of them the answer is
`422` saying to narrow the span. The row limit is checked by asking for one row more than the limit; the attachment
limits are checked from the records (count and size) **before any file is read** from the blob store. The package is
built off the async executor (it copies and sums up to 200 MiB) with the attached files moved into the zip, not copied;
there is no cap on packages built at once, so a deployment that fears many readers asking for the biggest span at the
same time limits them in front of the API. The limits are the defaults of `ExportLimits`, which a test lowers.

### Where the work lives

`AccountingExportService` (`application`) gathers the tables from the ports as plain data (`ExportTable` of text and
amount cells) and the attachments as `(path, bytes)`; `api` draws the files. The ports it needs are two methods on the
entries' port (`find_many`, `settlements_between`), a competence span on `FinancialEntryQuery`, a span and `unreversed`
on `TransferQuery`, and `AttachmentRepository::list_by_entries`; each is held to the same behavior by both adapters
(`contract::exports_the_books_of_a_span`). Names come from `ExportNames`, a second trait `RegisterReferences` implements
(like `EntryLookups`), so inactive records are named too.

## Consequences

- An export carries what the reader of the other areas can already see, gathered; it is an `accounting` read, so the
  accountant and every other role that reaches the area can download it, and nobody else.
- **The product does not keep the NF-e XML of a purchase it enters** ([ADR 0057](0057-purchase-entry-from-nfe-xml.md)): of
  what comes in, the package carries only what someone **attached** to an entry (a supplier's XML, a boleto or invoice PDF).
  The Contador page says it plainly. **Amended by [ADR 0076](0076-nfe-documents.md):** the fiscal documents the product itself
  issues (production, issued in the span) are in the package too, with `fiscal.csv`.
- A name is looked up per distinct party of the span: a company with many thousands of parties in one month pays that many
  reads. Acceptable at the sizes the limits allow; a batch read is the next step if it shows. The registers are read
  once per request, and the settlements' entries are read whole (installments included) only to name their rows.
- **An attached file whose bytes are gone** (the store lost it, or it was removed between the list and the read) makes the
  package answer `500` with the attachment named in the log, and asking again once it is gone from the list works: the
  record says the file exists, so a package without it would be a silent hole in what the accountant relies on.
- Files are bigger than compressed ones and the spreadsheet has no styles; nothing new is audited and nothing is built
  natively.
- Not in the package, on purpose: the audit log, the stock position, SPED files and any vendor's own layout (ADR 0067).

## Revisit when

A contributor maintains a public layout for an accounting vendor (a new `format` behind the same service), a package
needs to exceed the limits (stream it to the response), or the monthly package by e-mail (the next slice of
[ADR 0067](0067-accountant-and-reporting-plan.md)) needs the files attached instead of linked.
