---
title: "ADR 0034: Statement import (OFX) and reconciliation"
status: accepted
date: 2026-11-06
tags: [adr, finance, integrations, phase-2]
---

# ADR 0034: Statement import (OFX) and reconciliation

## Context

[ADR 0011](0011-bank-data-ingestion.md) chose OFX first and an aggregator later, behind the same tables. It
is accepted here with Phase 2's slice of it. A business reconciles when it checks that what the bank says
happened is what the books say happened: each line of the statement is either one of the settlements
([ADR 0030](0030-settlements.md)) or transfers ([ADR 0031](0031-transfers-and-balances.md)) already
recorded, or something that still has to be recorded, or something to leave out.

## Decision

### Statement lines

A **statement line** belongs to one bank account of the company: the day it was `posted_on`, a signed
`amount` in cents (positive came in, negative went out, never 0), a one-line `description` (at most 200
characters, white space collapsed), the bank's own reference when the file has one, and a **status**:
`pending`, `matched` or `ignored`. A matched line points at the **one** settlement or the **one** transfer it
is; a settlement or a transfer is matched by at most one line (unique indexes), except that a **transfer is two lines**, one in the statement of each
account, and is matched once in each. A move of the status is made from the status the line was read in:
if someone else moved it meanwhile it is refused (`409`), so two people, or a double submit, cannot both
make an entry of the same line. Lines are never edited and
never deleted: only the status moves (`pending` to `matched` and back, `pending` to `ignored` and back), and
every move is in the audit trail ([ADR 0008](0008-audit-trail.md)), as is the creation of each line.

### Import

`POST /companies/{id}/bank-accounts/{account}/statement-import` takes the OFX file **as it is** (the same
way as the CSV imports, [ADR 0025](0025-csv-import.md)): `?dry_run=true` only says what would happen, a file
with a line that is refused writes nothing, at most 5000 lines, and the account has to be an active one of
the company. The parser reads OFX 1.x (SGML) and 2.x (XML), in UTF-8 or in the Windows-1252 that Brazilian banks
still send, and only takes `STMTTRN` entries: `DTPOSTED`, `TRNAMT`, `FITID` and `MEMO`/`NAME`. An amount with three
decimals (`1.000`) is refused rather than read as 1, and a memo longer than a line keeps its first 200
characters. CSV with a column mapper, which ADR 0011 also lists, is **not** in this slice, and every
`STMTTRN` of a file goes to the account chosen (the file's own account and currency are not checked).

Importing the same file twice, or two files that overlap, must not double a line. Each line gets a
**fingerprint** from its day, amount, description, bank reference and its **position among identical lines
of the file** (two coffees of R$ 5,00 on the same day are two lines, and stay two when the file is sent
again), and the account keeps at most one line per fingerprint: a line that is already there is `skipped`.

### Reconciliation

- **Suggestions**: for a pending line, the settlements of the same account that are not reversed and not
  matched, whose **cash** ([ADR 0030](0030-settlements.md)) equals the amount, of a receivable when the line
  came in and of a payable when it went out, settled within **5 days** of the line; and the transfers into
  the account (for a line that came in) or out of it that are not reversed, not matched and have the same
  amount within 5 days. Nearest day first. They are suggestions: nothing is matched by itself.
- **Match** a pending line to one of them (the server checks all of the above again), **unmatch** it, **ignore**
  it (a fee that is already in the books some other way, a duplicate the bank sent) and **restore** an ignored
  one.
- **Create an entry from a line**: for a pending line that is not in the books, the person chooses a category
  (and may choose a party and a cost center and change the description) and the line becomes an entry with
  one installment for the amount, due on the day, **settled** the same day in the line's account, and is
  matched to that settlement. The steps are separate writes, each audited. What can be checked first is
  (the line is pending, the account and the payment method are usable) so that a refusal leaves nothing; a
  storage failure between the writes can still leave an entry that is not settled, and a settled one whose
  line is still pending.

All of it is in the `finance` area ([ADR 0026](0026-membership-roles-and-permissions.md)): the lines under
`statement-lines`, the import under `bank-accounts`.

### Where the rules live

The parser is in `infrastructure` (a file format is an adapter) and hands drafts to the use case, which knows
nothing of OFX, so a connector for an aggregator later feeds the same code. The matching rules are in the
domain (`StatementLine`) and the application (`ReconciliationService`); the candidates come from the ports of
the entries and the transfers, so both adapters answer alike.

## Consequences

- The statement can be reconciled line by line without leaving the product; what the bank says and what the
  books say are told apart by the status, and the balances ([ADR 0031](0031-transfers-and-balances.md)) are
  not touched by a line: only a settlement or a transfer moves money.
- A line the person marks `matched` to the wrong settlement is undone by unmatching it.
- No balance check against the statement's closing balance yet.
- Reversing a settlement or a transfer that a line is matched to does not touch the line: it stays matched to
  what is reversed, and has to be unmatched by hand. Nothing flags it yet.

## Revisit when

A bank's file needs a format this parser does not read, CSV statements are asked for, an aggregator feeds the
lines ([ADR 0011](0011-bank-data-ingestion.md)), or matching one line to several settlements (a deposit of
many receipts) or several lines to one settlement is needed.
