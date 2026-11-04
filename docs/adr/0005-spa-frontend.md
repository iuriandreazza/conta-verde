---
title: "ADR 0005: Separate SPA frontend"
status: accepted
date: 2026-11-04
tags: [adr, frontend]
---

# ADR 0005: Separate SPA frontend

## Context

The product is screen-heavy (registers, entries, reconciliation, quotes, POS) and must
also expose a public API for integrations and an installable mobile experience. The team is
small, so the frontend stack must have a large pool of components and contributors.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Server-rendered HTML from axum (askama/maud + HTMX) | One language, one toolchain | Rich interactions (reconciliation drag-and-drop, POS) become awkward; no offline/PWA story |
| Rust WASM frontend (Leptos/Yew) | One language | Smaller ecosystem for forms, tables and date pickers; slower iteration |
| Svelte SPA | Small bundles, pleasant | Smaller contributor pool than React |
| React + Vite + TypeScript SPA (chosen) | Largest ecosystem and contributor pool, mature table/form libraries, PWA tooling | Two toolchains (Rust and Node); a published API contract to maintain |

## Decision

The API (axum) is a standalone JSON service. The web app is a React + Vite + TypeScript
single-page app in `web/`, managed with pnpm, consuming only the public API
([ADR 0015](0015-public-api-and-webhooks.md)). There is no private API for the SPA:
anything the SPA can do, an integration can do.

Stack boundaries inside `web/`: TanStack Query for server state, TanStack Router or React
Router for routing, a headless component library plus a design system of our own (no
reuse of the reference product's visuals), and an OpenAPI-generated client so that DTO
changes break the build, not the user.

The frontend starts in Phase 1 (login and registers), when there are enough endpoints to
be useful. It ships as a PWA in Phase 8; there are no native apps.

The API serves the built SPA as static files in production so a single binary plus
PostgreSQL is a complete deployment.

## Consequences

- Two toolchains and a published API contract. In exchange, the API can serve other
  clients (integrations, connectors, PWA) unchanged.
- The SPA footer must link to the corresponding source (AGPL section 13).
- UI copy is in Portuguese for the end user; code, identifiers and docs stay in English.
- Visual identity is ours: no screenshots, icons or copy from the reference product.

## Revisit when

The SPA needs server-side rendering for public pages (payment page, quote approval link).
Those two pages are small enough to be rendered by the API directly, which is the planned
exception.
