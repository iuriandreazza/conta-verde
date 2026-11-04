---
title: "ADR 0001: License the project under AGPL-3.0-or-later"
status: accepted
date: 2026-11-04
tags: [adr, license]
---

# ADR 0001: License the project under AGPL-3.0-or-later

## Context

The goal is an ERP that anyone can use, host and modify, and that stays open. ERPs are
mostly consumed as hosted services, so a license that only triggers on distribution would
let a company run a modified fork as a SaaS without ever sharing its changes.

The project is a functional copy of a commercial product. The license must also make it
clear that we copy capabilities, not the brand: no trademark, logo, screenshots, copy or
data from the reference product may enter the repository.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| MIT / Apache-2.0 | Maximum adoption, no obligations for forks | A hosted fork can close the code; contributions may never come back |
| GPL-3.0-or-later | Strong copyleft on distribution | The SaaS loophole: hosting is not distribution |
| AGPL-3.0-or-later | GPL plus section 13: network users must be offered the source | Some companies ban AGPL dependencies; corporate adoption may be smaller |
| Business Source / SSPL | Protects a commercial offering | Not open source by the OSI definition; conflicts with the goal |

## Decision

AGPL-3.0-or-later. Every crate and package declares `license = "AGPL-3.0-or-later"`,
the `LICENSE` file is the full text, and the README states that the project is not
affiliated with or endorsed by any commercial product.

## Consequences

- Hosted forks must publish their changes. We accept a smaller corporate audience.
- The web app must expose a link to the corresponding source (section 13). The roadmap
  tracks it in Phase 2, when the SPA first ships to users.
- Dependencies must be compatible with AGPL-3.0: MIT, Apache-2.0, BSD, MPL-2.0 and LGPL
  are fine; SSPL, BUSL and proprietary SDKs are not. New dependencies are checked in review.
- Adapters for proprietary services (payment providers, fiscal gateways) are written
  against their public HTTP APIs, never by vendoring their SDKs.
- Brand hygiene: the reference product is named only in README, macro plan and ADRs as
  the functional reference; never in code identifiers, UI copy or screenshots.

## Revisit when

A contributor or sponsor needs a dual-license or CLA model. That would be a new ADR, and
re-licensing past contributions requires every contributor's consent.
