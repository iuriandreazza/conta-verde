---
title: "ADR 0020: Sidebar app shell, hand-drawn SVG charts and a sample-data boundary"
status: accepted
date: 2026-11-05
tags: [adr, frontend, ux, dataviz]
---

# ADR 0020: Sidebar app shell, hand-drawn SVG charts and a sample-data boundary

## Context

Phase 1 left the web app as a single narrow column under a top bar: the registers sat below their own forms,
nothing told people which company they were in, and there was no home page. Phase 2 (finance) needs a dashboard
and many more tables. The visual language is written down in [docs/design/](../design/README.md); this ADR records
the decisions behind it that are expensive to reverse.

## Options considered

| Option | Pros | Cons |
|--------|------|------|
| Keep the top bar and tabs | No change | Does not scale past three sections; the company is not visible |
| Sidebar shell (chosen) | Room for the Phase 2 to 8 sections, shows the company, collapses to a scrollable row on phones | Costs width on laptops |
| Chart library (Recharts, visx) | Less drawing code | A large dependency for four chart forms; its palettes and tooltips fight the validated color method |
| Hand-drawn SVG components (chosen) | No dependency, exact control of marks, tooltips and the table twin | We own the geometry code |
| Real numbers on the dashboard now | Honest | The financial data does not exist until Phase 2 |
| Sample data behind one module (chosen) | The layout and charts are reviewed now; Phase 2 swaps one import | A user could mistake it for real figures |

## Decision

- **Shell.** Signed-in pages live in a sidebar layout (company, sections, user), signed-out pages in a split layout.
  The landing page of a company is its dashboard (`/companies/:companyId`); a user with one company skips the picker.
  Registers show the list first and the create/edit form beside it (below it on narrow screens).
- **Charts are SVG components** in `web/src/components/charts/` with no chart library. Each follows the
  dataviz method: thin marks, a surface gap or ring, a legend for two or more series, selective direct labels, a
  hover and keyboard tooltip, and a table twin behind a "Ver como tabela" toggle. Categorical slots 1 and 2 of the validated
  palette are inflow/receivable and outflow/payable everywhere; they passed `validate_palette.js` in light mode.
  Overdue uses the critical status color only with an icon and a label.
- **Money stays decimal.** Chart data are decimal strings and every printed amount goes through `formatMoney`
  ([ADR 0003](0003-money-as-decimal.md)). `toDrawing` in `web/src/lib/chart.ts` is the single place a string becomes a
  `Number`, used only to place marks; axis ticks are round numbers chosen by `niceScale`, not figures derived from data.
- **Sample data boundary.** Every dashboard number comes from `web/src/dashboard/sampleData.ts`, and the page says so
  in a visible notice. `sampleData.test.ts` checks the hand-written totals in cents with `BigInt`. Phase 2 replaces that
  import with the finance endpoints and deletes the notice.
- **No dark mode yet.** Tokens are light only; chart tokens will get validated dark steps together with the app theme, not before.

## Consequences

- Phase 2 screens reuse `PageHeader`, `StatTile`, `ChartCard`, `LineChart` and `BarChart`; new chart forms need their own
  component and a re-run of the palette validator when they add series.
- The dashboard is not real until Phase 2: it must not be demoed as such.
- Adding a section means one more link in `CompanyNav`.

## Revisit when

- A chart needs zoom, brushing or hundreds of points: reconsider a library.
- The app gets a dark theme: add validated dark chart steps and a `data-theme` toggle.
