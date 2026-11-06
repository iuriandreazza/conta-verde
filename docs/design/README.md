---
title: Design system and screens
status: living
date: 2026-11-05
tags: [design, frontend, ux]
---

# Design system and screens

The visual language of the web app. The tokens live in [`web/src/styles.css`](../../web/src/styles.css);
the decisions behind the shell and the charts are in [ADR 0020](../adr/0020-app-shell-charts-and-sample-data.md).

The canvas in [`canvas/`](canvas/project/canvas.json) shows the design system and the screens side by side
(Claude Design format: one `.dc.html` per artboard). Open it as an artifact in Claude, or read the sources:

| Artboard | What it shows |
|----------|---------------|
| [`Main.dc.html`](canvas/project/Main.dc.html) | Colors, type, shape, components and chart rules |
| [`Dashboard.dc.html`](canvas/project/Dashboard.dc.html) | The company dashboard (layout; the page now shows real figures, see ADR 0043) |
| [`Registers.dc.html`](canvas/project/Registers.dc.html) | Parties list at full width, with the button that raises the form sheet |
| [`Login.dc.html`](canvas/project/Login.dc.html) | Signed-out split layout |

The artboards are static redraws of the real screens. When a token or a screen changes in the app, change the
artboard in the same PR; the canvas needs the Claude Design runtime (`support.js`), which is not in the repo.

## Principles

- **Quiet surfaces, one accent.** Neutral green-tinted grays, brand green for the primary action and the current place.
  Data takes blue and orange; red appears only for state, always with an icon and a label.
- **The list is the page.** Registers show the list at full width, and a record is created or changed in a bottom sheet over it (see [screen anatomy](screen-anatomy.md)); the content uses the width of the screen (up to 120rem). A row says what a record is under its name and keeps its columns for the figures: a table never scrolls sideways at 1440px.
- **Know where you are.** The sidebar always names the company and the section.
- **Numbers are text.** Money is a decimal string printed with `formatMoney`; charts only use numbers to draw.
- **Nothing relies on hover or color alone.** Each chart has a keyboard tooltip and a table view; states have icon and label.

## Tokens

| Group | Tokens |
|-------|--------|
| Brand | `--green` #1f7a4d, `--green-dark` #17603c, `--green-soft` #e6f4ec, `--forest` #0f2a1e (sidebar) |
| Neutrals | `--ink` #14201a, `--muted` #5d6b64, `--line` #e1e8e4, `--line-strong` #c9d5ce, `--page` #f5f7f6, `--surface` #fff |
| Feedback | `--danger`/`--danger-soft`, `--warning`/`--warning-soft` |
| Chart | `--series-1` #2a78d6 (inflow, receivable), `--series-2` #eb6834 (outflow, payable), `--status-critical` #d03b3b, `--chart-muted` #898781 |
| Shape | `--radius` 0.75rem cards, `--radius-sm` 0.5rem controls, pill badges; `--shadow`, `--shadow-lift` |
| Layout | `--sidebar-width` 16rem, content up to 120rem, side form 22rem |

The two series colors are slots 1 and 2 of the dataviz palette and passed `validate_palette.js` in light mode
(CVD ΔE 24.7, normal-vision ΔE 33.6, contrast over 3:1). Adding a third series means re-running it.

## Components

`PageHeader`, `Button` (primary, ghost, small, and `iconOnly` for the actions that repeat on every row: the label becomes the tooltip and the accessible name), `StatusBadge` (a dot and a word), `Notice` (error, warning, success), badges (`ok`, `warning`, `critical`,
`muted`), tables (`.num` right-aligns money), `StatTile`, `Sparkline`, `ChartCard` (title, legend, table twin),
`LineChart`, `BarChart`, segmented period filter, `CompanyNav`.

## Review

[`screens-review-2026-11.md`](screens-review-2026-11.md) compares the screens with the structure of a reference
overview page and checks charts and accessibility; it lists what each phase should pick up.

## Screen anatomy

[`screen-anatomy.md`](screen-anatomy.md) says where the menu, the page actions, the filters, the list and its buttons
go on every screen, so pages look and behave alike; new pages are built to it.

## Not yet designed

Dark mode, a date-range picker with a custom range, print styles for exports, and the screens of Phase 2 onward.
