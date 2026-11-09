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
| Menu | Two levels. A **rail** of areas (Painel, Comercial, Suprimentos, Financeiro, Cadastros, Relatórios, Configurações): an area with pages is a button that opens a **panel** beside the rail, the one with nothing under it (Painel, Relatórios) is a link. The panel holds the area's pages, the current one marked, and a fold for its registers (Financeiro has "Cadastros financeiros"). A control under the rail puts the panel away: an area then lists its pages in a flyout, and choosing one brings the panel back |
| Company | Named in the top bar, with a link to change it when there is more than one; at the head of the drawer on a phone |
| Mobile | The menu becomes a drawer opened from the top bar, in two screens: the areas, then the pages of the one chosen, with a way back and a close button. The content starts under the bar, never under a block of navigation |

Contents today: **Financeiro**: Contas a receber, Contas a pagar, Lançamentos, Fluxo de caixa, Transferências, Conciliação,
Recorrências, Cobrança, Inadimplência, and its registers (Categorias, Centros de custo, Contas bancárias, Formas de
pagamento, Prazos de pagamento). **Comercial**: Orçamentos, Vendas, Contratos, Comissões, Tabelas de preço.
**Suprimentos**: Compras, Estoque, Inventários, Relatórios de estoque. **Cadastros**: Clientes e fornecedores, Produtos e
serviços. **Configurações**: Membros.

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
so in a confirmation, and the page's list reads again (`useOnCreated`). Every register and document (finance, registers, members, quotes, orders, contracts, commission
rules, price tables) has a creator; a creator may hand back something that must be read first (the invitation's link), and the sheet
then stays on it until "Concluir". Closing or going back with something typed asks "Descartar edições?". The menu
shows an **Alt+letter** shortcut beside the usual items, which starts the same record from anywhere. `Sheet` is a dialog: the focus goes in and stays in,
Escape and the shade close it, and the focus returns to what opened it.

## 3. A report or overview page

Title in the top bar; below it one **filter row** (period navigator, grouping, account), then the figures, then the
charts, each chart in a card with its menu ("Ver como tabela", "Exportar") and an **"atualizado em"** stamp. Money in
and money out come as **two mirrored blocks** (receivable, payable), each with its create action and the two tiles
that matter: **overdue** and **due today**. Charts follow the dataviz rules in the [screens review](screens-review-2026-11.md).

**Relatórios de estoque** follow the same anatomy as **Relatórios**: a segmented switch (Valorização, Movimentações do período), one filter row (the day "Em", or the period navigator), the total and the products with stock as tiles, the eight biggest values as a bar chart with its table, and the full table; the movements report is one table with opening, in, out, adjustments and closing, and a totals row of the values.

## 4. Explanations behind an (i)

A screen shows what the person works with: data, the controls and the states that ask for something. **How a thing
works** (what a screen is for, what a card adds up, what a section's fields do, what changing a record leaves
untouched) sits behind an (i) button next to the title it explains, never in a paragraph under it. That keeps the
screen short for people who know it and one hover away for people who do not.

**How it behaves** (`InfoTip`): the (i) is a real button with the accessible name "Sobre …" (the title it explains).
Hover or keyboard focus-then-press shows the text, a press **pins** it, a second press, Escape (wherever the focus
is) or a press outside puts it away; the pointer can move onto the text without it vanishing. The text opens below
the button, lined up with its start (`align="start"`, where the (i) follows a title) or its end (where the (i) sits at
the right of a card); on a phone it takes the width of the window. It holds short paragraphs, no links to act on and
nothing needed to finish the task, because a person may never open it.

**Where it goes:**

| Explains | Component | Placement |
|----------|-----------|-----------|
| A screen | `PageHeader` `description` | Beside the page title. Facts about what the page shows (a customer, a charge) are the `subtitle`, always visible |
| A card or a section of a form | `HelpTitle` (`level` 2 or 3) | Beside the heading, with the heading's own spacing and rule kept |
| A chart | `ChartCard` `description` | Beside the chart's title |
| A control or a switch | `InfoTip` in a row with it (`action-with-tip`, `field-with-tip`) | At the side of the button, switch or field, never alone on a line |

**What stays in sight**, because hiding it would cost the person something:

- Errors, refusals and consequences to know before pressing (a sent quote that goes back to a draft when changed, a
  link that is shown only once, a contract that cannot be reopened) are `Notice`s or plain text, never an (i).
- Empty states: the call to action and its one line stay.
- Text that carries data: periods, totals, counts, the "Atualizado em" stamp, an account's details, a note on a record.
- A format hint directly under a field ("0,00", "até 6 casas decimais") stays as helper text: it is read while typing.
- A state that depends on the installation or the record ("provider is off", "this entry already has 20 files").

## 5. Components

| Component | State | Notes |
|-----------|-------|-------|
| `MenuButton` | built | One accessible menu button (`aria-haspopup`, arrows, Escape, focus back): powers the split button, "Mais filtros", "Ações em lote" and the row "Ações" |
| `RowActions` | deferred | The row's "Ações" menu over `MenuButton`. Rows keep their one-click icon buttons (each toggles a panel under the row, with `aria-expanded`); a menu would make those two clicks and rewrite most page tests, so it comes with a page that has more than four actions per row |
| `PeriodFilter` | built (Fluxo de caixa, Relatórios) | Previous, usual periods (month, three months, quarter, year), next, and the two dates; the dashboard uses it too, for its flow and sales cards |
| `FilterBar`, `SummaryStrip` | to build | Layout of the filter row; the state tiles as a radio group |
| `CreationProvider` | built (Lançamentos; the other registers follow) | The host of "Novo registro": `openCreate(kind)`, one creator per kind in `web/src/creation/`, `useOnCreated(kind, reload)` for the page |
| `Sheet` | built (Lançamentos; the other registers follow) | The bottom sheet for a form; its panel is an `<aside role="dialog">` named by its title |
| `InfoTip` | built (every screen, section and chart that explains itself) | The (i) button and its text, as in section 4. `CsvImport` keeps its card to the file field, the check and import buttons and the template link at the far end of the same row |
| `HelpTitle` | built | A card's or form section's heading with its `InfoTip` on the same line |
| `PageHeader` | built | The title, the page's `description` behind an (i) beside it, an optional visible `subtitle`, and the page-level actions |
| `Layout`, `AppTopbar`, `CompanyNav` | built | The top bar (menu on a phone, "Novo registro", user, sign out) and the grouped sidebar. A new section is one entry in `SECTIONS` of `CompanyNav` |

## 6. Order of work

1. Accessibility basics (done).
2. Top bar, `MenuButton` and the grouped sidebar (done).
3. The footer count and the status marker on **Lançamentos** and **Contas bancárias**, which also fixes the sideways scroll (done).
4. `PeriodFilter` on **Fluxo de caixa** and **Relatórios** (done).
5. The registers' forms move into `Sheet`, one page at a time (Lançamentos first).
6. `FilterBar` for the rest of the filter rows.
7. `SummaryStrip` on Lançamentos once the server answers the totals by state.
