---
title: "ADR 0030: Settlements of entry installments"
status: accepted
date: 2026-11-05
tags: [adr, finance, phase-2]
---

# ADR 0030: Settlements of entry installments

## Context

[ADR 0029](0029-financial-entries.md) made entries with installments and left open what an entry
becomes once an installment is paid: which fields still change, how a payment is undone, and how a
payment that is not the installment's face value (partial, with interest, a fine or a discount) is
recorded. Cash flow, bank balances and reconciliation (the rest of Phase 2) all read these records,
so they are fixed here.

## Decision

### A settlement

A **settlement** ("baixa") pays one installment of an entry, in part or in full, on a day, from a
bank account:

- `installment`: the number of the installment it pays;
- `settled_on`: a calendar date, the day the money moved (it may be before or after the due date,
  and is not tied to the competence date);
- `applied`: how much of the installment it settles, more than 0, in cents;
- `interest`, `fine`, `discount`: each 0 or more, in cents. They do not change what the installment
  is worth: they change what moved. Paying a 100 installment with 5 of interest and 2 of fine is
  `applied` 100, `interest` 5, `fine` 2, and 107 leaves the account; paying it with a 10 discount is
  `applied` 100, `discount` 10, and 90 leaves it;
- the **cash** is `applied + interest + fine - discount` and must be more than 0: a settlement that
  moves nothing is not a payment. It is what a receivable adds to the bank account and a payable
  takes from it. It is not stored: it is added up, like an entry's total;
- a **bank account** of the company, active (required), and an optional **payment method** of the
  company, active.

### Balances

An installment's **balance** is its amount minus the `applied` of its settlements that are not
reversed. It is never stored. A settlement that would take the sum of `applied` over the installment's
amount is refused (`409`), and so is one for an installment the entry does not have. Two requests
that settle the same installment at the same moment cannot both pass: the installment's row is
locked for the check and the write, in one transaction.

An installment is **open** (nothing settled), **partial** (some), or **settled** (all). Whether it is
also **overdue** depends on today's date and on the person's zone, so the API sends the amounts and
the web app, which knows the date where the person is, says "vencida"; a server-side filter for what
is overdue (next slice) takes today's date as a parameter and never reads a clock of its own.

### Reversal

A settlement is never edited or deleted. Undoing one is a **reversal**: `DELETE` on the settlement
marks it reversed (`204`), it stops counting in balances and cash, and it stays, with who did it and
when, in the audit trail ([ADR 0008](0008-audit-trail.md)). A reversed settlement is not settled
again: a new one is made. Reversing one already reversed is `404`.

### What an entry may change once it has settlements

While **any installment has a settlement that is not reversed**, the entry's **installments and kind
are fixed** (changing them is `409`): the dates and amounts are what the balances and the bank
movements were computed against, and a receivable cannot become a payable under money that moved.
Its description, category, cost center, party and competence date still change, since they only
classify. Reversing the settlements frees it again. (An entry with settlements is not deleted or
called off either: there is no delete, and cancelling comes with the next slice's reports.)

### Where the rules live

The settlement is its own domain type with its audit entity, but it is **part of the entry's
aggregate in storage**: the entry's port gets `settle`, `reverse` and `settlements_of`, because the
check against the balance, the write and the audit entry must be one transaction with the lock above.
The use case checks that the bank account and the payment method are usable through the same
references port as the entry ([ADR 0029](0029-financial-entries.md)).

### Roles

The routes are in the `finance` area ([ADR 0026](0026-membership-roles-and-permissions.md)):
`POST /companies/{id}/entries/{entry_id}/settlements`, `GET` the same path for its settlements
(reversed ones included), and `DELETE .../settlements/{settlement_id}` to reverse. An entry's response
carries, for each installment, what has been settled.

## Consequences

- Cash flow, balances per account and reconciliation read `entry_settlements`: the date and the cash
  of every unreversed settlement, against its bank account.
- Interest, fine and discount are recorded as given; the product does not compute them from a rate.
  Rates and automatic late fees are not part of this.
- Without a settlement an entry can still be corrected freely, which is why nothing here needs to
  change the registers.

## Revisit when

Transfers between accounts arrive (they are not settlements: they move money between two accounts of
the company with no entry), when a bank statement line has to be matched to a settlement
(reconciliation), or when a single payment has to settle several installments or entries at once.
