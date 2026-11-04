---
title: "ADR 0001: License the project under AGPL-3.0-or-later"
status: accepted
date: 2026-11-04
tags: [adr, license]
---

# ADR 0001: License the project under AGPL-3.0-or-later

## Context

The goal is an ERP that anyone can use and that stays open. ERPs are mostly consumed as hosted services.

## Options

- **GPL-3.0-or-later**: strong copyleft on distribution, but a company can host a modified version
  without ever distributing it, so it never has to share its changes.
- **AGPL-3.0-or-later**: same as GPL, plus section 13: users interacting with the software over a network
  must be offered the corresponding source.

## Decision

AGPL-3.0-or-later. Every crate and package declares `license = "AGPL-3.0-or-later"`.

## Consequences

- Hosted forks must publish their changes. Some companies avoid AGPL software, which may limit corporate adoption.
- The web app must expose a link to the source code (tracked in the roadmap).
- Dependencies must be license-compatible with AGPL-3.0 (MIT, Apache-2.0 and BSD are fine).
