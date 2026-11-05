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
- **The token is the secret; the address only binds it.** Accepting needs a session, and the
  session must be the **address the invitation was sent to** (`403` otherwise), so a link meant for
  one address is of no use to an account with another. But **there is no e-mail verification yet**,
  so owning an account with that address proves nothing about its mailbox: whoever holds the
  link and the address can register it (if it is free) and accept. The link is therefore **as
  sensitive as the e-mail that carries it**, and so is **the server's log while it is the only
  carrier** (below). What the check does give: someone who only knows or guesses an address cannot
  accept, since the 256-bit token comes only to the person who invites and to the mailbox. A
  sign-up **made through the link** (below) does rest on the mailbox: it needs the token and the
  address the invitation names. An ordinary sign-up with that address still proves nothing, and
  real e-mail verification is for a later ADR.
- The one route that needs no session, the preview, shows what the link is for (the company, the
  address and the role) to whoever holds the token, so that someone without an account yet can see
  what they are asked to join.
- Accepting **claims the invitation first**, in one atomic step that also checks it is still open
  and unexpired, and **then** makes the membership. An accept that races a withdrawal, or the end
  of the invitation's time, loses, and no membership is made behind a refusal. If the membership
  then cannot be written, the claim is given back so the person can try again; if the user is
  already a member (a second accept that raced the first), the company is theirs and the answer is
  success. The two are not one transaction because the identity and invitation stores are
  separate ports.

### The e-mail

- `EmailSender` is the port of [ADR 0014](0014-notifications.md) (now accepted, for e-mail): one
  message to one address. The invitation's message is composed in the application, in Portuguese,
  and says who invited, to which company, the link and how long it lasts.
- **Sending never fails the invitation.** The invitation exists whether or not the message went
  out, and the answer says `emailed: true` or `false`; the web page shows the link either way so
  the person who invited can pass it on.
- With `SMTP_URL` set the message goes through SMTP ([ADR 0014](0014-notifications.md)); otherwise the default adapter **writes the message to the server's log**, sends nothing and says so (it answers that the message was not sent, so the page shows the link instead of claiming an e-mail went out). The log then
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

### Signing up through the link

`POST /auth/register` takes either `company_name` (an ordinary sign-up, which makes a company) or
`invitation`, the token (never both: `422`; neither: `422`). Through an invitation the account is
made **with no company of its own**, joins the invited company in the invited role and is signed in
at once, so the new person lands in that company. `MemberService::join` checks everything before it
stores anything (the password, that the invitation is open, that the address is the one invited,
`403` otherwise), then registers the user (`IdentityRepository::register_user`, a user with no
company) and accepts as above. A refusal leaves no account and the invitation open; an address that
already has an account is `409` and signs in instead. If the invitation is withdrawn in the instant
between the check and the claim, or the membership cannot be written, the account exists with no
company: in the second case the claim is given back, so signing in and accepting at the invitation's
page works; in the first the person asks for a new invitation. The sign-up page reads the preview, hides the company name and fixes the address to the
invited one; a link that no longer works leaves an ordinary sign-up, with the reason shown.

## Consequences

- **An address can be squatted.** With no e-mail verification, someone who registers an address
  before its owner does keeps the real person from signing up with it (`409`), and from accepting
  an invitation sent to it (`403`). Without the token the squatter gains nothing else, but the
  invitee is locked out until verification exists.
- The names that go into the message (who invited, the company) are typed by people and are put on
  **one line** (every control character becomes a space), and an e-mail address with a control
  character is refused, so neither can add a header to a message or forge a line in a log once a
  real mail server sends it.
- There is no rate limit on the preview, which needs no session; a token of 256 bits makes
  guessing pointless, and a limiter in front of the API is the remedy for the rest, as for the CNPJ
  and CEP lookups.
- Not here: handing a company over (an owner is not
  invitable), password reset, and the web app hiding what a role cannot use (the API refuses it
  already).

## Revisit when

Joining without creating a company, background jobs exist (the send moves to one, with a retry and
a "resend"), the list of members needs removing or re-roling people, or a company needs invitations
that outlive seven days (a setting per company).
