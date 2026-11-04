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

The frontend is not part of the foundation PR; it starts in Phase 1, when there are enough endpoints to be useful.

## Consequences

Two toolchains (Rust and Node) and a published API contract. In exchange, the API can serve other clients (mobile, integrations) unchanged.
The SPA footer must link to the source code (AGPL section 13).
