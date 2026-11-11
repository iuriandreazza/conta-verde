---
title: Leaving a reference product
status: living
date: 2026-11-11
tags: [migration, guide, import]
---

# Leaving a reference product

How to bring a company over from another ERP to conta-verde without retyping it. It speaks of "a reference product's export" because
conta-verde is not affiliated with any product and never names one in its code or documents; if your old system can export a spreadsheet
of what is below, this guide applies.

> **What was and was not tried.** The importers are tested with files made for the tests. The conversion script
> (`scripts/export-to-entries-csv.py`) was written against the layout of one cloud ERP's "contas a pagar/receber" and "extrato financeiro"
> exports; no other product's export was ever tried, and the maintainers have no way to promise a layout stays as it was. Each step
> below can be **checked without writing anything** (`Verificar arquivo`), which is how you find out.

## The order

Each step needs the one before it, because a row names what it points at.

1. **A copy of the old system's reports on the day you stop.** Keep the balance of every bank account, the open receivables and
   payables, and the stock valuation as the old product shows them on the cut-over day. They are what you compare against at the end.
2. **Close the old system's day.** Choose the **cut-over day** and stop entering things in the old product on it. Everything below is
   "as of" that day.
3. **Registers.** Customers and suppliers (`Cadastros > Clientes e fornecedores`, the card *Importar de um arquivo CSV*), products and services (`Cadastros > Produtos e
   serviços`, the same card), categories, cost centers, bank accounts, payment methods and payment terms (`Financeiro > Cadastros
   financeiros`; small enough to type, or to re-create from the default category template). Products need a **SKU** (the stock file names products by it) and a **cost price**.
   Importers only **create**: a row whose document or SKU already exists is skipped, never overwritten, so a file can be sent again.
4. **Bank accounts' opening balances.** On `Financeiro > Cadastros financeiros > Contas bancárias` set each account's **opening balance** on the cut-over day (it is signed).
5. **Open entries (what is still to be received or paid).** Convert the old product's receivable/payable reports with
   `scripts/export-to-entries-csv.py` (see its header) or write the file by hand, and import it on `Financeiro > Lançamentos`
   (*Importar*) ([ADR 0064](../adr/0064-entry-import-from-a-file.md)). A row carries a `reference` of your choosing (the old system's own number is
   best): sending the file again finds what it already made. Paid history is optional; bring it only for the span you want to report on.
6. **Stock balances.** On `Suprimentos > Estoque > Importar saldos` ([ADR 0094](../adr/0094-stock-balance-import.md)): a file with `sku`, `quantity` and
   `unit_cost` per product, a name for the import and the cut-over day. It opens the stock of products that have no movements yet, valued at
   the cost you give, and refuses anything else, so it cannot double a running stock.
7. **Check.** Compare with step 1:
   - the balance of every bank account (`Financeiro > Cadastros financeiros > Contas bancárias`) against the old balance,
   - the open receivables and payables (`Relatórios`, the overdue list, and `Financeiro > Inadimplência`) against the old lists,
   - the stock valuation (`Suprimentos > Relatórios de estoque`) against the old stock valuation.

   A difference is almost always a row skipped (an existing document or SKU) or refused (a product that already had stock movements),
   a quantity or cost typed with a thousands separator, or an account with no opening balance. Every importer says which row and why.
8. **Close the month.** On the accountant's page (`Contador`) lock the last month of the old system, so the migrated history cannot
   be changed by accident ([ADR 0069](../adr/0069-period-lock.md)).

## The stock file

```csv
sku,quantity,unit_cost
CAFE-500,10,5.00
PAO-1,3,
```

- `sku` names the product (case does not matter). `quantity` has at most three decimals. `unit_cost` is what **one unit** cost, up to six
  decimals; blank takes the product's cost price, and a product with neither is refused.
- A comma may be the decimal mark (`12,5`); a value like `1.234` is refused as ambiguous: write `1234`, or `1,2340`.
- A product that appears twice, an unknown SKU, a service (services have no stock) or a negative quantity refuses the **whole** file, with the
  row of every problem; fix and send again.
- Quantity `0` opens nothing and is skipped.
- Sending the same file with the same name twice adds nothing. Another file under another name, for a product that already has its
  opening balance, is refused: to correct a balance, make an **inventory count**.

## What does not come over

Fiscal documents already issued by the old product (keep its XML files: they are your record), commissions history, price lists by
customer, attachments, anything in the old product's own banking or credit features, and user accounts (invite people again).
conta-verde does not guess: whatever a file does not say stays empty.

## When something is wrong after the cut-over

A wrongly opened balance is corrected with an inventory count (the movements of a file cannot be reversed by hand); a wrong entry is reversed or cancelled in the app (nothing is deleted).
If the whole migration was wrong, restore the backup taken before you started: see the operations guide once it exists
([ADR 0092](../adr/0092-one-point-zero-hardening-plan.md)), and take that backup **before step 3**.
