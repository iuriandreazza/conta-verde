---
title: "ADR 0005: Separate SPA frontend"
status: accepted
date: 2026-11-04
tags: [adr, frontend]
---

# ADR 0005: Separate SPA frontend

## Decision

The API (axum) is a standalone JSON service. The web app is a React + Vite + TypeScript single-page app in `web/`, managed with pnpm.
React was chosen over Svelte for its larger ecosystem and contributor pool.

The frontend started in Phase 1, once authentication and a first list endpoint existed. It calls `/api/*` on its own origin; a proxy
(Vite in development, a reverse proxy in production) strips the prefix and forwards to the API, so the session cookie
([ADR 0007](0007-cookie-sessions.md)) stays same-origin and the API needs no CORS.

## Consequences

Two toolchains (Rust and Node) and a published API contract. In exchange, the API can serve other clients (mobile, integrations) unchanged.
The SPA footer must link to the source code (AGPL section 13); the link is configurable with `VITE_SOURCE_URL` so a fork points at its own code.
