---
title: "ADR 0007: Server-side sessions in an HttpOnly cookie"
status: accepted
date: 2026-11-05
tags: [adr, auth, security]
---

# ADR 0007: Server-side sessions in an HttpOnly cookie

## Context

Phase 1 needs authentication so that `company_id` stops being trusted input ([ADR 0004](0004-company-scoped-tenancy.md)).
The first client is a browser SPA served from the same origin as the API ([ADR 0005](0005-spa-frontend.md)).

## Decision

- Passwords are hashed with Argon2id. Hashing runs on the blocking thread pool, so it never stalls the async workers.
- A login creates a random 32-byte session token. Only its SHA-256 digest is stored, so a leaked database does not leak usable sessions.
- The token travels in a `cv_session` cookie: `HttpOnly`, `SameSite=Lax`, `Path=/`, and `Secure` when `COOKIE_SECURE=true`.
  Scripts cannot read it, and browsers do not attach it to cross-site POSTs. Together with JSON-only bodies this is the CSRF defense; there are no CSRF tokens.
- Sessions last 30 days and are revocable: logout deletes the row. Expired rows are swept whenever a new session is created.
- Unknown emails still pay for an Argon2 verification (against a decoy hash), and a malformed email gets the same answer as a wrong password.
- A user who is not a member of a company gets `404`, the same as a company that does not exist, so company ids cannot be probed.
- The web app and the API are served from one origin (Vite dev proxy locally, a reverse proxy in production), so no CORS is needed.

## Alternatives considered

- **JWT access and refresh tokens**: stateless and convenient for mobile clients, but revocation is awkward and the browser must store the token where scripts can reach it. Revisit when a non-browser client appears; the `SecretService` port keeps that change local.

## Consequences

- Every authenticated request costs one indexed lookup on `sessions`.
- Plain-HTTP deployments must leave `COOKIE_SECURE` off, which is acceptable only on loopback. Put a TLS-terminating proxy in front before exposing the API.
- Not covered yet: password reset, invitations to existing companies, rate limiting of login attempts, and roles other than owner.
