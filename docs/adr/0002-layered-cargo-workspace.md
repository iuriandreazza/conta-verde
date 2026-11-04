---
title: "ADR 0002: Layered Cargo workspace"
status: accepted
date: 2026-11-04
tags: [adr, architecture]
---

# ADR 0002: Layered Cargo workspace

## Context

The business rules of an ERP (installment splitting, settlement, average cost, tax rules)
outlive its frameworks. We want them testable without a database or an HTTP server, and
we want the compiler, not a code review, to stop layers from depending on each other the
wrong way. The project also expects many external integrations (banks, payment providers,
fiscal gateways, messaging), each of which must be swappable and fakeable in tests.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Single crate with modules | Least ceremony | Nothing prevents `domain` from importing `sqlx`; layering erodes |
| One crate per layer (chosen) | Dependency direction enforced by Cargo; fast unit tests in `domain` and `application` | A feature touches up to four crates |
| One crate per bounded context (sales, finance, …) | Scales with modules | Premature now; cross-context rules (sale → receivable) are the common case early on |
| `dyn Trait` ports with `async-trait` | Familiar DI style | Extra allocation and a dependency; generics give the same testability |

## Decision

One crate per layer, dependencies pointing inwards (Clean Architecture with ports and adapters):

| Crate | Responsibility | May depend on |
|-------|----------------|---------------|
| `domain` | Entities, value objects, business rules. No I/O. | nothing but `uuid` (and `rust_decimal` from Phase 2) |
| `application` | Use cases (`*Service`) and the port traits they need (`*Repository`, later `PaymentProvider`, `Notifier`, …). | `domain` |
| `infrastructure` | Adapters implementing ports: PostgreSQL via sqlx, in-memory fakes, HTTP clients for external services. | `application`, `domain` |
| `api` | HTTP (axum), DTOs, authentication middleware, wiring in `main.rs`. | all of the above |

Use cases are generic over their ports (`PartyService<R: PartyRepository>`) instead of
`dyn` objects; port methods return `impl Future + Send`.

SQL uses runtime-checked `sqlx::query`, not the `query!` macros, so builds do not need a
live database or committed offline data. The PostgreSQL adapter is covered by `#[ignore]`d
tests that CI runs against a service container.

Bounded contexts (finance, sales, inventory, fiscal) live as modules inside each layer
crate until a context grows enough to justify its own crates. That split will be a new ADR.

## Consequences

- Adding a feature touches up to four crates. The vertical-slice PR convention in the
  macro plan keeps that manageable.
- A broken query is found by tests, not by the compiler, so every repository method needs
  a Postgres test.
- Every port has at least two adapters (real and in-memory/fake) that must behave the
  same, including ordering, which doubles some test effort but keeps use-case tests fast.
- External-service adapters never leak their types into `application`: the port speaks
  domain types only.

## Revisit when

A single layer crate takes more than a minute to compile incrementally, or two bounded
contexts need to evolve independently (different release cadence or maintainers).
