---
title: "ADR 0016: Motion for animation, vendored Lucide Animated icons"
status: accepted
date: 2026-11-05
tags: [adr, frontend, ux]
---

# ADR 0016: Motion for animation, vendored Lucide Animated icons

## Context

The SPA ([ADR 0005](0005-spa-frontend.md)) is used for repetitive data entry all day. Small, fast feedback (a pressed
button, an error that appears, a new row that lands) makes that work feel responsive, but animation must never
slow people down, hide content, or ignore the system's reduced-motion setting.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| CSS transitions only | No dependency, no bundle cost | Hard to do enter/exit, springs and per-row staggering; icon animations become hand-written keyframes |
| Motion (`motion` on npm, formerly Framer Motion) | Springs, gestures, exit animations, `MotionConfig reducedMotion`; the icon set below is built on it | About +150 kB raw, +49 kB gzip measured on the base app (84 to 133 kB gzip); one more dependency |
| Static icons (`lucide-react`) | Small, well known | No animation on hover or press |
| Lucide Animated icons (MIT, built on Motion) | Ready-made animated Lucide icons, ownable code | Copied into the repo, so upstream fixes are not picked up automatically |

## Decision

- Use **Motion** for component animation and keep it decorative: it never carries meaning.
- Use **Lucide Animated** icons, **vendored** into `web/src/components/icons/` (copy-paste is the library's own model).
  The icons are installed from the public registry (`https://lucide-animated.com/r/<name>.json`) after checking each file:
  one dependency (`motion`), imports limited to `motion/react`, `react` and a `cn` helper, no network or DOM access.
  Only the changes the app needs are made: the `cn` import points at `web/src/lib/utils.ts` and the Next.js `"use client"` line is dropped.
- `MotionConfig reducedMotion="user"` wraps the app, and the CSS honors `prefers-reduced-motion`.
- Pages animate **on enter only**. Tests turn animation off globally (`MotionGlobalConfig.skipAnimations`).
- Rules of thumb: animations last under 250 ms, buttons respond to press, errors slide in instead of shifting the layout,
  and nothing blocks input while it plays.

## Consequences

- The JS bundle grows by about 49 kB gzip. If it matters, switch to `LazyMotion` with the `m` component, which would
  mean rewriting the vendored icons too.
- Adding an icon means running the same check on its registry file before committing it.
- Each vendored icon wraps its SVG in a `<div>`. That is fine inside links and flex rows (CSS keeps it inline), but it is
  outside the HTML content model for `<button>` and not allowed in a `<p>`: wrappers that hold an icon must be `<div>`s.
- The logo and favicon are original artwork (`web/public/favicon.svg`, mirrored by `LogoMark` in `web/src/components/Logo.tsx`);
  they must not resemble any commercial product's identity.

## Revisit when

Bundle size or the first paint on slow devices becomes a problem, or Motion changes its license or API in a way that breaks the icons.
