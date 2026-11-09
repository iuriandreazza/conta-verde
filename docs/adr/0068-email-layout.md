---
title: "ADR 0068: One layout for every e-mail"
status: accepted
date: 2026-11-09
tags: [adr, email, design]
---

# ADR 0068: One layout for every e-mail

## Context

The application sends nine e-mails (invitation, invitation accepted, password reset, password changed, address
confirmation, quote, payment reminder, payment receipt, purchase order). Each was a hand-written string of plain text,
so none looked like the product, a link was a long line the client might break, and a figure (an amount, a due date)
was a line like any other. The design canvas "Layout dos e-mails" draws one layout for all of them.

## Options considered

1. **Hand-written HTML in each use case.** Nine copies of the same markup, each able to forget to escape a name that a
   person typed.
2. **A template engine and files.** A dependency and a second place to read what an e-mail says, for nine short messages.
3. **A content model drawn twice.** Each use case says what the e-mail contains (`EmailContent`: heading, paragraphs,
   labelled details, a table of items, a notice, a note, a button) and one place draws it as the plain-text part and as
   the HTML part.

## Decision

Option 3.

- `EmailContent` (in `application`) is the only thing a use case builds. `to_text` and `to_html` draw it; the HTML
  escapes every string and makes a link only for an `http(s)` address. The layout is tables and inline styles, since mail
  clients do not read a stylesheet; its colors are the design tokens.
- `EmailSender::send_content` takes a content. By default it sends the text alone; `BrandedEmailSender`, a wrapper the
  API puts around the queue, adds the HTML, with the footer's link to the source code (`SOURCE_URL`, AGPL section 13,
  upstream's repository by default). Tests and the log adapter keep working on the text.
- `EmailMessage` gains `html: Option<String>`. SMTP sends `multipart/alternative` when it is there and plain text
  otherwise.
- A `send_email` job takes an optional fourth argument, the HTML; a job queued before this change has three and still
  runs (migration `20261212000000_job_email_html.sql` widens the `CHECK`).

## Consequences

- Every e-mail looks the same and reads well in a mail client that shows HTML; one that does not still gets the text.
- A new e-mail is a content, not markup. Changing the look is one file.
- The layout has a wordmark and no logo image: mail clients do not draw SVG, and a hosted PNG would be a request to our
  server on every open.
- The text part is no longer worded exactly as before (each block is a paragraph or `Label: value`), so tests assert
  that the facts are there, not the shape.

## Revisit when

A client needs a dark theme, a logo image, or the company's own name and colors in the header.
