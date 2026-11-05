---
title: "ADR 0027: Invitations"
status: accepted
date: 2026-11-05
tags: [adr, security, tenancy, notifications]
---

# ADR 0027: Invitations

## Context

[ADR 0026](0026-membership-roles-and-permissions.md) gave a membership a role, but the only way to
belong to a company was to create it. [ADR 0007](0007-authentication-and-authorization.md) decided
that people join by an e-mailed invitation. This ADR sets how an invitation works, who can send
one, what its link may give to whoever holds it, and how the e-mail goes out
([ADR 0014](0014-notifications.md)).

## Decision

### The invitation

- It offers **an e-mail address a role in a company**, never the role of owner (owning a company is
  what creating one makes you). It is made by a member who reaches `members` for writing: an owner
  or an admin, in any role but owner's.
- A company has **at most one open invitation per address**; a second one is `409`. An open one
  that has **expired** is withdrawn and replaced by the next invitation to that address, since the
  person asking again is the way to get a new link. Nobody who is **already a member** is invited
  (`409`).
- It **expires after 7 days**. It is used once, or withdrawn, or expires; those are told apart in
  the answer (`410`, "was already used or withdrawn, or has expired: ask for a new one") from a
  link that never existed (`404`).
- It is recorded on its own table with who invited and when. It is **not audited**
  ([ADR 0008](0008-audit-trail.md)): it is a membership write, which the trail does not hold.

### The link

- The link is `{PUBLIC_URL}/invitations/{token}`, in the web app. The token is 256 random bits; the
  database keeps only its **SHA-256 digest**, so a leaked database leaks no usable link, and the
  link is shown to the person who invites **once**, in the answer that creates it. Listing the
  open invitations never shows it.
- **A link alone gives nothing.** Accepting needs a session, and the session must be the
  **address the invitation was sent to** (`403` otherwise). A link that is forwarded, guessed or
  read in a log therefore makes nobody a member; the person who holds it only learns what it is for
  (the company, the address and the role), which the preview, the one route that needs no session,
  shows so that someone without an account yet can see what they are asked to join.
- Accepting makes the user a member in the invitation's role and marks the invitation used. The
  membership is written **first** and the mark after: if the mark is lost the invitation stays open
  and accepting again finds the membership and finishes, instead of an invitation that is used up
  and made nobody a member. The two are not one transaction because the identity and invitation
  stores are separate ports.

### The e-mail

- `EmailSender` is the port of [ADR 0014](0014-notifications.md) (now accepted, for e-mail): one
  message to one address. The invitation's message is composed in the application, in Portuguese,
  and says who invited, to which company, the link and how long it lasts.
- **Sending never fails the invitation.** The invitation exists whether or not the message went
  out, and the answer says `emailed: true` or `false`; the web page shows the link either way so
  the person who invited can pass it on.
- The default adapter **writes the message to the server's log**, sends nothing and says so (it answers that the message was not sent, so the page shows the link instead of claiming an e-mail went out). The log then
  holds the link, which, by the rule above, accepts nothing for anyone but the address it names.
  A real SMTP adapter and a local mail catcher come next, behind the same port.
- Sending is inline in the request, which ADR 0014 says it should not be: there are no background
  jobs yet ([ADR 0009](0009-postgres-only-background-jobs.md)). A failed or slow send does not
  fail or hold the invitation back beyond the send's own timeout; moving it to a job is part of
  that ADR's work.

### The web app

A members page (who belongs, the open invitations with "withdraw", and a form that invites and
shows the link once) for the roles that reach `members`, linked from the company's menu only for
them. The page a link leads to shows what the invitation is without a session, then asks to sign in
or create an account **with that address** (the sign-in and sign-up pages return to the invitation
afterwards, and only to an invitation: a crafted `next` goes nowhere else), then accepts.

## Consequences

- A person who has no account creates one first, and the sign-up asks for a company name, so
  they end up with an empty company of their own beside the one they joined. The accept page says
  the name can be their own. Joining straight from the invitation, without creating a company, is
  the first thing to improve.
- There is no rate limit on the preview, which needs no session; a token of 256 bits makes
  guessing pointless, and a limiter in front of the API is the remedy for the rest, as for the CNPJ
  and CEP lookups.
- Not here: removing a member, changing a member's role, handing a company over (an owner is not
  invitable), password reset, and the web app hiding what a role cannot use (the API refuses it
  already).

## Revisit when

Joining without creating a company, background jobs exist (the send moves to one, with a retry and
a "resend"), the list of members needs removing or re-roling people, or a company needs invitations
that outlive seven days (a setting per company).
