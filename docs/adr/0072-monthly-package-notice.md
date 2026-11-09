---
title: "ADR 0072: The monthly package notice, by e-mail to the accountants"
status: accepted
date: 2026-11-09
tags: [adr, accounting, email, jobs]
---

# ADR 0072: The monthly package notice, by e-mail to the accountants

## Context

[ADR 0067](0067-accountant-and-reporting-plan.md) decided that each company may ask the product to tell its accountants,
every month, that the previous month's package is ready, and why that message is **text with a link and not the files**.
[ADR 0071](0071-accountant-exports.md) made the package. This ADR records what the schedule is, the pass that runs it and
the one rule that makes a month go out once.

## Decision

### The schedule

`PackageSchedule` (`domain`) is a switch and a **day of the month from 1 to 28**, one per company, replaced as a whole by
`PUT /companies/{id}/accounting/package-schedule` (`enabled` and `day`, both said every time: a request that leaves one out
is `422`, so it cannot switch the notice by forgetting what it was) and read by `GET`, which also says **how many members it
would reach**. A company that never chose has it **off, on the 5th**. A write is audited (`AuditEntity::PackageSchedule`,
the company's id as the entity's, the changed fields only) in the same transaction, the same choice again writes nothing,
and `GET .../package-schedule/history` lists who chose what. It is in the `accounting` area: the accountant, the owner and
admins write it; every other role reads it ([ADR 0067](0067-accountant-and-reporting-plan.md)). The table checks the day
(`CHECK day BETWEEN 1 AND 28`) because the domain does.

### Who, when and for which month

- **Who:** every member whose role is `accountant`, and **nobody else**. No address can be typed in: the link needs a login,
  so a recipient is someone who has one. A company with no accountant sends nothing, and the page says so, with a link to
  invite one.
- **When:** an **hourly pass** (`announce_monthly_packages` in `main.rs`, `PackageScheduleService::send_due`) announces the
  **previous month** to the companies whose switch is on and whose day has come: from the chosen day to the end of the month,
  so a server that was down on the day still sends it when it is back. The day is `customer_today()` (UTC turns the day at
  21:00 in Brazil), the one place besides the other hourly passes where the server reads the clock; `send_due` takes the day
  and the moment, so tests move them. Turning the switch on in the middle of a month sends the previous month that day.
- **Once per company and month:** a row in `package_sends` (primary key `(company, month)`, the month's first day) is
  **claimed by one statement before anything is queued** (`INSERT … ON CONFLICT DO NOTHING`, true for the one caller that
  inserted it), so however often the pass runs, and on however many instances, a month goes out once. The claim is
  operational, like a job: not audited, no actor.
- **When nothing could be queued the claim is given back**, so the next pass tries again. When **some** messages were queued
  and others not, the claim **stays**: a second pass would send the first ones twice. A company with no accountant is not
  claimed, so inviting one later in the month still announces it.

### The e-mail

It goes through the queue ([ADR 0059](0059-job-queue.md)) like every e-mail (`Ports::Emails`), so a failed delivery is retried
by the worker, and it is drawn in the shared layout ([ADR 0068](0068-email-layout.md)). It says that the package of
`outubro de 2026` of the company is ready, names the company (the recipient works for it, and the invitation says the same)
and has one button, **"Abrir o pacote"**, to
`{PUBLIC_URL}/companies/{id}/accounting?month=2026-10`: the Contador page, which asks for a login and opens its downloads
on that month. **It carries no figure, no file and no download link that works without a login**; its small print says so.
Why not attach the files: a package is megabytes and the transport is text; why not a link with a token: it would download
a company's books for whoever holds the message ([ADR 0067](0067-accountant-and-reporting-plan.md)).

### The page

The Contador page gets a card, **"Aviso mensal aos contadores"**: a switch and the day, who it reaches (and "no accountant:
nothing is sent" with a link to invite one), and the explanation behind the (i). A reader sees what is set and no form.
The downloads card opens on the month in the link's `?month=` when it is a month, and otherwise on the one closed last.

## Consequences

- Delivery is **at most once** per company and month, by design: the claim comes first, so a crash between the claim and
  the last message leaves some accountants (in the worst case all) not told, and nothing sends that month again. Claiming
  after sending would tell some accountants twice instead. The Contador page is always there: the notice is a convenience,
  not the only way to know.
- **Each company's turn is isolated**: an error reading one company (its members, its name) is logged and counted and the
  others go on. What can fail is read **before** the claim, so once the month is claimed only the sending is left, and a pass
  that queued nothing gives the claim back. If giving it back fails too, the month is lost for that company (logged with the
  company's id).
- **With no way to send e-mail** (the log-only setup, no `SMTP_URL`) the pass does nothing and claims nothing, so the month
  is still there when there is one; the card still says whom the notice would reach.
- The catch-up window ends with the month: a day-28 schedule in February has one day, and a server down from the 28th into
  the next month never sends that one.
- The day is the customers' (`customer_today()`, UTC turns it at 21:00 in Brazil), so a notice for day D first goes out at
  about 21:00 Brazil time on D, not in the morning.
- The link of the e-mail leads to a login-gated page: signing in brings the person back to it (`/login?next=`, which takes
  an invitation's page or a page of a company and nothing else), with the month in the address.
- The pass reaches **every** company of a database, so a test that shares one asserts on its own recipients, not on the pass's
  report.
- Nothing about the package is stored but the schedule and the claims; the files are made when the accountant asks, exactly
  as [ADR 0071](0071-accountant-exports.md) says.
- The month is announced even if it is not closed: the notice says the package is ready to download, not that the books are
  final. Closing is still the accountant's act ([ADR 0069](0069-period-lock.md)).

## Revisit when

An accountant wants to opt out of the notice of one company without leaving it; the files must be attached to the e-mail
(the transport would have to carry them); a company wants the notice only once the month is closed; or a time zone other
than the customers' becomes the one that turns the day.
