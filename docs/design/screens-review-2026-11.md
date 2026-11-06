---
title: Screens review against a reference overview page
status: living
date: 2026-11-06
tags: [design, frontend, ux, accessibility, dataviz]
---

# Screens review against a reference overview page

A review of the web app's screens (Phases 0 to 2) with three lenses: design critique, accessibility (WCAG 2.1 AA)
and the dataviz method (palette validated with its script). The page structure of a cloud ERP's overview page
served as a **structural reference only**: what follows describes how it is organised, in our own words. Nothing
of its identity, copy or assets is used, and the notes and screenshots of the reference stay outside the repository.

Method: the app was run locally (API, Vite, a seeded company with 24 entries and 15 settlements, localhost only)
and looked at in a browser at 1440 px and 375 px; contrast ratios were computed, the series colors were run
through `validate_palette.js`, and `document` structure was inspected. The tokens and principles reviewed are in
[`README.md`](README.md) and [ADR 0020](../adr/0020-app-shell-charts-and-sample-data.md).

## What the reference page does (abstract)

| Area | Structure |
|------|-----------|
| Shell | A top bar (global search, one "new record" menu, help, account) over a sidebar of **collapsible groups**, with a control to collapse the sidebar |
| Header | Title, a privacy toggle that masks every amount, and dismissible onboarding banners above the content |
| Scope | One view selector and a link to the full list, above the figures |
| Money in / money out | Two mirrored blocks, each with a heading, a "create" action and two tiles: what is **overdue** and what **falls due today** (with what is left of the month). Numbers are green (in) and red (out) |
| Cards row | Three cards side by side: financial accounts (total plus one item per account, with a count of pending reconciliations), a short daily cash flow (in, out, balance) and 12 months of sales bars |
| Card chrome | An overflow menu, an explicit "refresh" button and a "last updated" stamp on each chart |

What not to copy: onboarding banners that push the figures below the fold, charts refreshed by hand, a legend that
rests on color alone, and a short-window line chart that is flat and empty with little data.

## Where the app differs, and what is worth adopting

Owners: **Phase 3** = the sales session (ADR 0037 puts the dashboard slice, with real figures replacing the sample
data, and the navigation in its hands); **this review** = contained changes done here.

| # | Screen | Difference | Why it matters | Owner |
|---|--------|------------|----------------|-------|
| S1 | Shell | 15 flat links in the sidebar, no groups; Phase 3 adds more. `Painel` and `Relatórios` share one icon | Finding a section gets slower with every phase; the same icon for two destinations | Phase 3 (nav) |
| S2 | Shell | No global "new" action: creating an entry means going to Lançamentos first | The most frequent task is two steps away from everywhere | Phase 3 (nav) |
| S3 | Shell, mobile | The sidebar becomes a block above the content: logo, company, a sideways-scrolling row of 15 links (12 hidden off-screen) and the user take about 40% of the first screen | Phone users see the title and nothing else, and cannot see most destinations | this review |
| S4 | Dashboard | Receivable and payable are mixed in generic tiles; the reference keeps them as two mirrored blocks with overdue and due today, each with a create action | Receivable and payable are the two questions a small business asks first; "overdue" and "due today" are the actions | Phase 3 (dashboard) |
| S5 | Dashboard | No accounts card (balance per account, pending reconciliation count linking to Conciliação) | The reconciliation backlog is invisible from the home page | Phase 3 (dashboard) |
| S6 | Dashboard | Sample data with a warning notice, although Phase 2 is merged | Home page is not true; ADR 0037 already schedules it | Phase 3 (dashboard) |
| S7 | Dashboard, cash flow | Figures carry no "as of" stamp | People need to know how fresh a balance is once it is real | Phase 3 (dashboard) |
| S8 | All money screens | No way to mask amounts | Screen sharing and shared desks; cheap with `formatMoney` as the single printer, but touches every page | proposal, later |
| S9 | Cash flow, Reports, Dashboard | Three period idioms: a 3/6/12 months segmented control, free `De`/`Até` dates (each in its own filter card), nothing | The same question is asked three ways; the README lists the range picker as "not yet designed" | this review (a shared `PeriodFilter` with presets) |
| S10 | Bank accounts | Seven columns in about 680 px at 1440 px: the table **scrolls sideways** and `Situação` is cut off. The README principle says a table never scrolls sideways at 1440 px | Principle broken on a core register | this review |
| S11 | Entries | The filter `Mostrar` shows "A receber e a pag" (truncated); the description cell wraps its badge, kind and category over three lines | The main list of the product reads as cramped | this review |
| S12 | Reports | Bars of 0 print `R$ 0,00` beside the baseline, doubling the labels | Noise: a zero is the baseline | this review (chart) |
| S13 | Registers | Filters live inside the list card, but Cash flow, Reports and Reconciliation put them in a separate card above | Same job, two layouts | note, fixed with S9 where it overlaps |

Worth keeping: the sidebar always names the company; `PageHeader` everywhere (15 of 15 pages); the list-and-form
split; `StatusBadge` with dot and word; reconciliation's empty state, which says what to do next.

## Dataviz review

Run with the validator on 2026-11-06 (surface `#fcfcfb`).

| Rule | Result |
|------|--------|
| Palette | Slots 1 and 2 (`#2a78d6`, `#eb6834`): **pass**, CVD ΔE 24.7, normal-vision ΔE 33.6, contrast ≥ 3:1. Dark steps (`#3987e5`, `#d95926`) also pass against `#1a1a19`, so the dark theme can adopt them as they are |
| One axis, never dual | **Pass** in `LineChart` and `BarChart` |
| Categorical order, color follows the entity | **Pass**: blue is always inflow or receivable, orange outflow or payable |
| Legend with two series, direct labels | **Pass**: `Legend` plus end labels when they fit |
| Table twin, keyboard | **Pass**: `ChartCard` toggle; arrows on the line chart; focusable groups on bars |
| Status color with icon and label | **Pass**: overdue uses the glyph and a word |
| `--chart-muted` (`#898781`) for the projected balance | The validator **fails** it as a categorical slot (chroma 0.009, reads gray). It is not a categorical slot here, it is the de-emphasis gray, so it is legal as long as the projection has another encoding: a **dashed** stroke is the usual one for a forecast and is what the chart should use (proposed, minor) |
| Zero-valued bars labelled | Fails "no number where the baseline says it" (S12) |
| Line charts start at zero | `niceRange` always includes 0, so inflow and outflow in the sample (36 to 54 thousand) are squeezed into the top third. Allowed for lines, but readability would gain from a padded range when no value is negative. Not changed here: it alters how every chart reads, so it needs a decision |
| Dark mode | Not designed (ADR 0020); the 8 hex values and `#fff`/`#f8faf9` literals in `styles.css` that bypass tokens (below) must become tokens first |

## Accessibility review

| Check | Result |
|-------|--------|
| Text contrast | `--muted` on page 5.2:1, on white 5.6:1; warning and danger badges 5.4:1 and 5.7:1; sidebar secondary text 8.4:1. **Pass** |
| Placeholder text | `#7f8c85` on white is **3.5:1**, below 4.5:1 (S14) |
| Series colors as graphics | `#2a78d6` 4.4:1, `#eb6834` 3.2:1 against white: pass for marks (3:1); never used as text |
| Focus | A 2px outline on everything (light on the sidebar); inputs swap the outline for a green border plus a halo. **Pass** |
| Page titles | `<title>` is `conta-verde` on every route (WCAG 2.4.2): a screen reader user cannot tell pages apart in tabs or history (S14) |
| Bypass blocks | No skip link: 15 sidebar links come before the content on every page (2.4.1) (S14) |
| Landmarks, language, one `h1` per page | **Pass** (`lang="pt-BR"`, `aside`, `nav` named, `main`, `footer`) |
| Touch targets | Only the footer link (inline text) is below 24 px: exempt. Row actions are 32 px |
| Reduced motion | **Pass** (`MotionConfig`, CSS media query) |
| Mobile | The nav row is reachable only by sideways scrolling (S3) |

S14 is the review's accessibility slice: per-page titles, a skip link and a placeholder color that reaches 4.5:1.

## Design-system audit

- **Tokens bypassed**: `#fff` is typed 8 times where `--surface` exists; `#f8faf9` (table header and hover) 3 times with no token; the sidebar uses 5 translucent whites. These are the first things a dark theme would trip on. Proposed tokens: `--surface-subtle` (`#f8faf9`) and `--surface-hover`.
- **Docs disagree**: the README says content goes "up to 120rem" in the principles and "up to 76rem" in the token table. Fix in this review's docs slice.
- **Canvas is behind**: the artboards cover the dashboard, parties and login. Entries, cash flow, reports, reconciliation and bank accounts have no artboard, and the README still says "the screens of Phase 2 onward" are not designed.
- **Components worth extracting** once there are two users: `PeriodFilter` (S9), a `MoneyBlock` for the receivable/payable pair (Phase 3).

## Plan

Slices of this review, each with web tests, `pnpm check`, a browser check and its own PR:

1. **This document and the README fixes** (docs only).
2. **Accessibility basics (S14)**: page titles through `PageHeader`, a skip link in `Layout`, placeholder contrast.
3. **List layouts (S10, S11) and the zero labels (S12)**.
4. **Shared `PeriodFilter` (S9)** for Cash flow and Reports, if the first three land cleanly.
5. **Mobile shell (S3)**: only if it can be done without touching the nav items.

Left to Phase 3, deliberately: S1, S2, S4 to S7 (they change the navigation or the dashboard's data), and S8
(masking amounts) as a later proposal. Checked on 2026-11-06: no open pull request touched `web/`.

## Second pass: parity with the reference's way of working

Goal: someone who already uses a cloud ERP of this kind should know where things are and how to start a record
without being taught. Described in our own words; no wording, logo or screen of the reference is used.

| What a person expects | Where it is in conta-verde |
|---|---|
| A "new" button in the top bar that starts any record | `AppTopbar` New menu; each item opens a bottom sheet over the current page, never navigates |
| Revenue and expense as separate starting points | "Nova receita" / "Nova despesa" (Alt+R / Alt+D), the entry sheet preset to the kind |
| Receivables and payables as their own lists | "Contas a receber" / "Contas a pagar" in Financeiro: the entries list with `?tipo=` in the address |
| Every list has search, filters, an order and "load more" | `usePagedList` pages; filters in one row above the table |
| Creating on a list page opens the same sheet | `openCreate` from the page's button, `useOnCreated` reloads and flashes the new row |
| Keyboard shortcuts for the common creations | Alt+letter hints in the New menu (`MenuItem.shortcut`); Alt+R and Alt+D replace the generic entry's Alt+L, and recurrence lost its Alt+R |
| Closing a half-filled form asks first | `SheetGuard`: "Descartar edições?" |

Not matched because they need the API first: a period (month) filter on entries, a strip with the totals by state
above the list, numbered pages (the API is cursor-paginated), and bulk actions on selected rows. Export, print and
a reports menu in the action row are the next front-end slice.
