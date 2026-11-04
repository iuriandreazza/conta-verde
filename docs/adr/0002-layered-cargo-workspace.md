---
title: "ADR 0002: Layered Cargo workspace"
status: accepted
date: 2026-11-04
tags: [adr, architecture]
---

# ADR 0002: Layered Cargo workspace

## Context

The business rules of an ERP outlive its frameworks. We want them testable without a database or HTTP server,
and we want the compiler to prevent layers from depending on each other the wrong way.

## Decision

One crate per layer, with dependencies pointing inwards (Clean Architecture with ports and adapters):

| Crate | Responsibility | May depend on |
|-------|----------------|---------------|
| `domain` | Entities, value objects, business rules. No I/O. | nothing but `uuid` |
| `application` | Use cases and the port traits they need (e.g. `PartyRepository`). | `domain` |
| `infrastructure` | Adapters implementing ports: PostgreSQL via sqlx, in-memory. | `application`, `domain` |
| `api` | HTTP (axum), DTOs, wiring in `main.rs`. | all of the above |

Use cases are generic over their ports (`PartyService<R: PartyRepository>`) instead of using `dyn` objects,
which keeps dependency injection explicit and avoids the `async-trait` crate.

SQL uses runtime-checked `sqlx::query`, not the `query!` macros, so builds do not need a live database or committed offline data.
The PostgreSQL adapter is covered by an `#[ignore]`d test that CI runs against a service container.

## Consequences

- Adding a feature touches up to four crates, which is more ceremony than a single crate.
- A broken query is found by tests, not by the compiler.
