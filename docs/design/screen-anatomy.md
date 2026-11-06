---
title: Screen anatomy
status: proposed
date: 2026-11-06
tags: [design, frontend, ux, navigation]
---

# Screen anatomy

How a screen of conta-verde is organised: where the menu, the page actions, the filters, the list and its buttons go.
People who come from other cloud ERPs should find things where they expect them, so the **organisation** follows the
common layout of that kind of product (see the [screens review](screens-review-2026-11.md)); the look stays ours
(tokens in `web/src/styles.css`, [ADR 0020](../adr/0020-app-shell-charts-and-sample-data.md)), and no wording, name or
asset of any other product is used. Page and menu names are our own plain Portuguese.

This is the spec the new pages (Phase 3 on) are built to, and the one the existing pages are moved to, in slices.

## 1. Shell

| Part | Rule |
|------|------|
| Top bar | Always visible. Left: logo and the **title of the page**. Right: a global **"Novo registro" menu** (new entry, new sale, new party…, only what the role may write), search when it exists, the user menu (company switch, sign out) |
| Sidebar | Groups, never a flat list: **Início**, **Financeiro**, **Comercial** (Orçamentos, Vendas), **Cadastros**, **Relatórios**, **Configurações**. A group opens in place and the group of the current page stays open; the current link is marked. A control collapses the whole sidebar to icons |
| Company | Named in the sidebar header, and in the user menu when there is more than one |
| Mobile | The sidebar becomes a drawer opened from the top bar: the content starts under the bar, never under a block of navigation |

Group contents today: **Financeiro**: Lançamentos, Fluxo de caixa, Transferências, Conciliação, Recorrências.
**Cadastros**: Clientes e fornecedores, Produtos e serviços, Categorias, Centros de custo, Contas bancárias,
Formas de pagamento, Prazos de pagamento. **Relatórios**: Relatórios. **Configurações**: Membros.

## 2. A list page

From top to bottom, in this order:

1. **Action row.** The primary action is a button with a menu of kinds when there is more than one ("Novo lançamento ▾":
   a receivable, a payable). Next to it, secondary menus and buttons: **Importar** (when the register has an import),
   **Exportar**, **Relatórios**. All are `MenuButton`s or `Button`s and read like buttons, never like links.
2. **Filter card.** One row: the **period filter** (previous, the period as a menu, next, and the dates) when the list has a
   date, a **search box with its button**, the closed-value selects that matter most, and **"Mais filtros"**, a menu that
   adds the rest as fields. The filters are one state object (`usePagedList`) and "Limpar filtros" resets it.
3. **Summary strip** (lists that have states): clickable tiles with the count and the total of each state: overdue,
   due today, to come, settled, period total. The chosen tile has an accent edge and filters the list; it is a radio
   group, so it works by keyboard. Needs totals from the server: a list without that endpoint has no strip.
4. **Table.** A checkbox column and a bar "N selecionados" with an **"Ações em lote"** menu where bulk actions exist.
   Sortable headers (`aria-sort`). The row says what a record is under its name (party, category, as chips) and keeps
   its columns for figures. A **status marker on the row's left edge** says "overdue" in the critical color and, as
   always, also in words (the status pill). The last cell holds the row's actions: up to four **icon buttons with a tooltip** (edit, settle, history,
   attachments), and an **"Ações" menu** (`MenuButton`) when there are more.
5. **Footer.** "Mostrando N registros" and the pager. The API paginates by cursor ([ADR 0018](../adr/0018-list-endpoints-search-filter-sort.md)),
   so the pager is **"Carregar mais"** with the count; numbered pages would need a different API and are not planned.
6. **Totals of the period**, collapsible, under the table, when the list has amounts.

A record is created or changed in a **bottom sheet** (`Sheet`) that rises over the page that is open, as in the
products people come from: the list is full width and has **no add-record panel beside it**. The action row's primary
button ("Novo lançamento") and a row's edit button open it; a save or "Fechar" closes it and the list shows what
changed (the new row flashes, a "… adicionado." notice sits above the list). The global "Novo registro" menu (`CreationProvider`) raises the sheet of the register chosen **over the page that is
open, without leaving it**: a creator component holds that register's form and saves it, the host closes the sheet, says
so in a confirmation, and the page's list reads again (`useOnCreated`). Registers that have no creator yet still go to
their own page. `Sheet` is a dialog: the focus goes in and stays in,
Escape and the shade close it, and the focus returns to what opened it.

## 3. A report or overview page

Title in the top bar; below it one **filter row** (period navigator, grouping, account), then the figures, then the
charts, each chart in a card with its menu ("Ver como tabela", "Exportar") and an **"atualizado em"** stamp. Money in
and money out come as **two mirrored blocks** (receivable, payable), each with its create action and the two tiles
that matter: **overdue** and **due today**. Charts follow the dataviz rules in the [screens review](screens-review-2026-11.md).

## 4. Components

| Component | State | Notes |
|-----------|-------|-------|
| `MenuButton` | built | One accessible menu button (`aria-haspopup`, arrows, Escape, focus back): powers the split button, "Mais filtros", "Ações em lote" and the row "Ações" |
| `RowActions` | deferred | The row's "Ações" menu over `MenuButton`. Rows keep their one-click icon buttons (each toggles a panel under the row, with `aria-expanded`); a menu would make those two clicks and rewrite most page tests, so it comes with a page that has more than four actions per row |
| `PeriodFilter` | built (Fluxo de caixa, Relatórios) | Previous, usual periods (month, three months, quarter, year), next, and the two dates; the 3/6/12 months control of the dashboard is the Phase 3 dashboard slice's to replace |
| `FilterBar`, `SummaryStrip` | to build | Layout of the filter row; the state tiles as a radio group |
| `CreationProvider` | built (Lançamentos; the other registers follow) | The host of "Novo registro": `openCreate(kind)`, one creator per kind in `web/src/creation/`, `useOnCreated(kind, reload)` for the page |
| `Sheet` | built (Lançamentos; the other registers follow) | The bottom sheet for a form; its panel is an `<aside role="dialog">` named by its title |
| `PageHeader` | exists | Moves its title into the top bar; keeps the description and, for now, the page-level actions |
| `Layout`, `AppTopbar`, `CompanyNav` | built | The top bar (menu on a phone, "Novo registro", user, sign out) and the grouped sidebar. A new section is one entry in `SECTIONS` of `CompanyNav` |

## 5. Order of work

1. Accessibility basics (done).
2. Top bar, `MenuButton` and the grouped sidebar (done).
3. The footer count and the status marker on **Lançamentos** and **Contas bancárias**, which also fixes the sideways scroll (done).
4. `PeriodFilter` on **Fluxo de caixa** and **Relatórios** (done).
5. The registers' forms move into `Sheet`, one page at a time (Lançamentos first).
6. `FilterBar` for the rest of the filter rows.
7. `SummaryStrip` on Lançamentos once the server answers the totals by state.
