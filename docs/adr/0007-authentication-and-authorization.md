---
title: "ADR 0007: Authentication and authorization"
status: accepted
date: 2026-11-04
tags: [adr, security, tenancy]
---

# ADR 0007: Authentication and authorization

## Context

Phase 1 turns the unchecked `company_id` of [ADR 0004](0004-company-scoped-tenancy.md)
into a real boundary. The reference product has one login that can access several
companies, permission profiles per user, and an accountant who is invited into companies
they do not own. Integrations (Phase 8) need a non-interactive credential. The SPA runs on
the same origin as the API in production but on a different port in development.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Stateless JWT in local storage | Simple for SPAs, no session table | No revocation, XSS exposure, token refresh dance |
| Server-side session + HttpOnly cookie (chosen for users) | Revocable, immune to XSS token theft, works with the same-origin SPA | Needs a session table and CSRF protection |
| External identity provider (OAuth/OIDC only) | No password handling | Self-hosters would need a provider; adds a dependency for the smallest installs |
| API keys (chosen for integrations) | Simple for machines, scoped per company | Must be hashed at rest and rotated |

## Decision

- **Users** authenticate with e-mail and password hashed with Argon2id. A successful login
  creates a row in `sessions` and sets an HttpOnly, Secure, SameSite=Lax cookie. Sessions
  expire by inactivity and absolutely; logout deletes the row. State-changing requests
  carry a CSRF token issued with the session.
- **Companies and memberships**: a user has zero or more memberships, each with one role:
  `owner`, `admin`, `financial`, `sales`, `stock`, `accountant` (read-only across the
  books, Phase 6), `viewer`. Permissions are coarse per module; fine-grained permissions are
  not planned before 1.0.
- **Scoping**: the `company_id` in the URL stays for explicitness; the middleware rejects
  any request whose session has no membership in that company. Use cases receive a
  `CompanyId` that has already been checked.
- **Integrations** use API keys bound to one company and a role, stored hashed, sent as a
  bearer token ([ADR 0015](0015-public-api-and-webhooks.md)).
- **Invitations** are e-mailed tokens that create a membership on acceptance
  ([ADR 0014](0014-notifications.md)).
- OIDC login is an optional later addition behind the same `sessions` model.

## Consequences

- One session table and a CSRF check in the API; the SPA needs no token handling.
- Every audited write records the user from the session ([ADR 0008](0008-audit-trail.md)).
- Self-hosted single-company installs still get a login screen; there is no anonymous mode.
- Password reset and e-mail verification depend on the e-mail notifier, so Phase 1 ships
  the SMTP adapter early.

## Revisit when

Users ask for SSO (OIDC adapter), two-factor authentication, or per-record permissions
(for example a seller who only sees their own customers).
