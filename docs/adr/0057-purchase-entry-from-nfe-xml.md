---
title: "ADR 0057: Purchase entry from a supplier's NF-e XML"
status: accepted
date: 2026-11-06
tags: [adr, purchases, inventory, fiscal, phase-5]
---

# ADR 0057: Purchase entry from a supplier's NF-e XML

## Context

Most goods arrive with the supplier's NF-e, and typing it again is where mistakes come from. This slice reads the XML,
shows what it says, and enters it as a purchase that arrived whole: the order ([ADR 0053](0053-purchase-orders.md)), its
payable, and the stock entries ([ADR 0054](0054-stock-movements.md)). It is **parsing and validation only**: nothing
here asks SEFAZ or any network anything, and whether the invoice is authorized is not checked (the `nfeProc`'s protocol
is not read).

## Decision

### Reading the XML

`parse_nfe` (`infrastructure`, `nfe_xml.rs`, on `roxmltree`) turns the bytes into a plain `InvoiceDraft` of text, like
`parse_ofx` does for a statement. It is the only place that knows XML. The file is **limited to 1 MiB before the parser
sees it**, must be UTF-8, and is parsed with **no document type declaration allowed** (so no entity, internal or
external, is ever declared or expanded: a file that points at `file:///etc/passwd` or expands onto itself is refused as
"not a valid XML") and a limit on the number of nodes. The messages name what is wrong with the file and never repeat
what the parser said. Only a **normal invoice of model 55** is taken (`finNFe` 1 or absent): complementary notes,
adjustments, returns and consumer notes are refused. It reads the issuer (CNPJ or CPF), the number, series and day,
each item (code, EAN, description, NCM, unit, quantity, values, discount, freight, insurance, other expenses, and the IPI,
ICMS ST, FCP ST, import tax and ICMS the item shows), the totals and the installments (`cobr/dup`).

### Checking it, in the domain

`Invoice::new` holds every rule, and a draft that breaks one is `422`: the access key is 44 digits with the **right check
digit** (modulo 11, weights 2 to 9) and **names the issuer, series and number** the XML says; each item's quantity is above
0 with **at most 3 decimals** (what stock keeps); the items' products and discounts add up to the totals; the value of the
invoice is `products - discount - ICMS exemption + ICMS ST + FCP ST + freight + insurance + other + import tax + IPI`; and
the installments, when there are any, are above 0, never go back in time and **add up to the value of the invoice**.

### What an item costs

An item costs its products less its discount **plus the charges the invoice puts on it** (freight, insurance, other
expenses, IPI, ICMS ST, FCP ST, import tax), and **what is left to reach the value of the invoice** (charges that are on
the invoice but on no item) is shared over the items in proportion, cut to the cent, the last item taking the rest, so
**the costs add up to what the supplier is paid, exactly**. The ICMS of the item is shown and is not part of the cost (it
is recovered by who can). A purchase line is built so that it is worth exactly its item's cost: the unit cost is the cost
over the quantity **rounded up** to six decimals, and the few cents that makes it worth too much are a discount.

### Entering it

`POST /companies/{id}/purchase-imports/preview` takes the XML as the body and says what it is, the supplier registered
with the issuer's registry number, the product each item's code finds (**the product whose SKU is the code**, compared
without regard to case), the cost of each item and whether it was entered. Nothing is written.
`POST /companies/{id}/purchase-imports?category_id=...` takes the XML too and the rest as parameters (cost center,
payment term, the day the goods arrived, the supplier when it is not the one the issuer finds, and the product of each item
where the code finds none, separated by commas): it makes the purchase and receives all of it.

- The **supplier must be registered** (`422` otherwise; registering it is a different screen, and a party made from the
  invoice's data would skip what the party register checks). An item with no product is `422` naming its number.
- The order carries **the access key** (`purchase_orders.invoice_key`, never changed, a unique index per company), and
  **the order is the claim**: a key makes at most one order. Entering the same file again, or at the same moment from two
  places, finds it and **finishes what a failure left undone** (approve, receive, payable, stock entries: each step is
  the idempotent one of ADR 0053/0054) and answers `200` with `already_entered`; the first time is `201`.
- The receipt is **paid on the installments the invoice gives** (`purchase_receipt_installments`: a receipt may say when its
  payable falls due, the parts adding up to the receipt, instead of the order's payment term); an invoice that gives none
  uses the payment term named, or one installment on the day the goods arrived.
- The audit trail has the order's creation (with the key), its approval and the receipt, in the name of whoever entered it.

### Roles

`purchase-imports` is in the `purchases` area.

## Consequences

- `DocumentReferences` gains `party_with_document` and `product_with_code`; `PurchaseService` gains `receive_scheduled`,
  `preview_invoice` and `enter_invoice`.
- The exit criterion is two tests on both adapters and through the API: a purchase XML creates the payable and the stock,
  and a sale then takes the stock at the average cost (`a_purchase_xml_creates_payables_and_stock_and_a_sale_takes_it_at_the_average_cost`).

## Known limits

An order entered from an invoice is **not edited**, and cannot be received on its own (`409`): it is received by entering
the invoice. Cancelling one that was never received frees its key (the unique index leaves cancelled orders out), so the
invoice can be entered again. The access key's issuer field may hold letters (an alphanumeric CNPJ). A purchase has at
most 100 items, fewer than an NF-e may have (990). Nothing checks that the invoice is authorized (its protocol and signature
are not read) or that it is a purchase and not a sale the company issued itself (`tpNF`), so a company's own invoice can be
entered if the company is registered as a supplier of itself; a false file that occupies a real key is undone by cancelling
its order.

The unit of the invoice is not converted to the product's; the quantity is taken as it is. A supplier's invoice is not
matched to an order made earlier. The invoice's own XML is not kept (the access key, the issuer and the notes are); an
accountant who needs the file keeps it, until the attachments of Phase 6 take it. An item whose code is not a SKU is
mapped by hand each time.

## Revisit when

The XML has to be kept with the purchase, a company wants an item's code remembered per supplier, units must be converted,
an invoice must be matched to an open order, or the authorization of the invoice must be checked.
