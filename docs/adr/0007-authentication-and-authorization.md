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

## Implementation notes

First Phase 1 slice (`feat/auth-tenancy`). Where it differs from the decision above, the decision stays the target:

- Implemented: Argon2id, sessions keyed by a SHA-256 digest of the token (the raw token only lives in the cookie),
  HttpOnly + SameSite=Lax cookie (`Secure` via `COOKIE_SECURE=true`, since development is plain HTTP), logout deletes the row,
  membership check on every `/companies/{company_id}` route.
- Not yet: the CSRF token (for now SameSite=Lax plus JSON-only bodies is the defense), inactivity expiry (sessions last
  an absolute 30 days), invitations (roles on memberships are in: [ADR 0026](0026-membership-roles-and-permissions.md)), password reset, audit trail.
- The SPA calls `/api/*` on its own origin through a proxy (Vite in development), so the dev port difference needs no CORS.
- Login for an unknown or malformed email still verifies against a decoy hash, so response time does not reveal registered emails.
  `POST /auth/register` does answer `409` for a registered email; closing that needs e-mail verification.
- Concurrent Argon2 runs are capped at the CPU count, since each one holds about 19 MiB.

## Revisit when

Users ask for SSO (OIDC adapter), two-factor authentication, or per-record permissions
(for example a seller who only sees their own customers).
