#!/usr/bin/env python3
"""A smoke check of every area of the API through a running server and a real database.

It walks the paths that cross the layers where a mistake would show first: a purchase that makes stock and a payable, a sale
that takes the stock and makes a receivable, a quote answered through its public link, a charge and its public payment page,
the reports, the password reset that goes through the job queue, and a second company that must not see any of it. It is the
check that the server still works when the database is held to the row-level policies (ADR 0093), and the smoke check of a
restored backup (docs/operations).

    export $(cat .env.example | xargs) CNPJ_LOOKUP=off BIND_ADDR=127.0.0.1:3100
    cargo run -p conta-verde-api &
    uv run scripts/e2e-areas.py http://127.0.0.1:3100

It makes its own accounts (random addresses, random passwords that are never printed) and leaves them in the database.
"""

import argparse
import http.cookiejar
import json
import secrets
import urllib.error
import urllib.request


class Client:
    def __init__(self, base):
        self.base = base.rstrip("/")
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def call(self, method, path, body=None):
        data = None if body is None else json.dumps(body).encode()
        request = urllib.request.Request(self.base + path, data=data, method=method)
        if data is not None:
            request.add_header("Content-Type", "application/json")
        try:
            with self.opener.open(request) as response:
                return response.status, response.read()
        except urllib.error.HTTPError as error:
            return error.code, error.read()

    def json(self, method, path, body=None, expect=200):
        status, payload = self.call(method, path, body)
        assert status == expect, f"{method} {path}: expected {expect}, got {status}: {payload[:300]!r}"
        return json.loads(payload) if payload else None


def check(condition, message):
    if not condition:
        raise AssertionError(message)
    print(f"  ok  {message}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("api", help="the API's address, e.g. http://127.0.0.1:3100")
    arguments = parser.parse_args()

    tag = secrets.token_hex(4)
    password = secrets.token_urlsafe(18)
    owner = Client(arguments.api)
    owner.json(
        "POST", "/auth/register",
        {"name": "Dona", "email": f"dona-{tag}@example.test", "password": password, "company_name": f"Mercearia {tag}"},
        expect=201,
    )
    company = owner.json("GET", "/auth/me")["companies"][0]["id"]
    base = f"/companies/{company}"

    def create(path, body):
        return owner.json("POST", f"{base}/{path}", body, expect=201)

    print("The registers")
    revenue = create("categories", {"name": "Vendas", "dre_group": "gross_revenue", "cash_flow_group": "operating"})["id"]
    purchases = create("categories", {"name": "Compras", "dre_group": "operating_expenses", "cash_flow_group": "operating"})["id"]
    create("categories", {"name": "Transferências", "dre_group": "transfers", "cash_flow_group": "transfer"})
    customer = create("parties", {"name": "Cliente", "document": "11.222.333/0001-81", "email": "cliente@example.test", "is_customer": True, "is_supplier": False})["id"]
    supplier = create("parties", {"name": "Fornecedor", "document": "12.ABC.345/01DE-35", "is_customer": False, "is_supplier": True})["id"]
    product = create("products", {"kind": "product", "name": "Café", "sku": "CAFE-1", "unit": "un", "sale_price": "10", "minimum_stock": "5", "active": True})["id"]
    account = create("bank-accounts", {"kind": "cash", "name": "Caixa"})["id"]
    term = create("payment-terms", {"name": "30/60", "installments": [{"days": 30, "percent": "50"}, {"days": 60, "percent": "50"}], "active": True})["id"]

    print("A purchase makes stock and a payable")
    order = create("purchase-orders", {
        "supplier_id": supplier, "ordered_on": "2026-11-01", "category_id": purchases, "payment_term_id": term,
        "lines": [{"product_id": product, "description": "Café", "quantity": "10", "unit_price": "4"}],
    })
    owner.json("POST", f"{base}/purchase-orders/{order['id']}/approve")
    receipt = owner.json("POST", f"{base}/purchase-orders/{order['id']}/receipts", {"received_on": "2026-11-02", "items": [{"line": 1, "quantity": "10"}]}, expect=201)
    owner.json("POST", f"{base}/purchase-orders/{order['id']}/receipts/{receipt['id']}/finish")
    positions = owner.json("GET", f"{base}/stock/positions")["items"]
    check(any(p["product_id"] == product and p["quantity"] == "10" for p in positions), "the stock holds the 10 received")

    print("A sale takes the stock and makes a receivable")
    sale = create("sales", {
        "customer_id": customer, "sold_on": "2026-11-03", "category_id": revenue, "payment_term_id": term,
        "lines": [{"product_id": product, "description": "Café", "quantity": "3", "unit_price": "10"}],
    })
    completed = owner.json("POST", f"{base}/sales/{sale['id']}/complete")
    check(completed["status"] == "completed" and completed["receivable_id"], "the sale is completed with its receivable")
    positions = owner.json("GET", f"{base}/stock/positions")["items"]
    check(any(p["product_id"] == product and p["quantity"] == "7" for p in positions), "the stock holds 7 after the sale")
    check(len(owner.json("GET", f"{base}/stock/alerts")["items"]) == 0, "7 is not below the minimum of 5")

    print("A quote is answered through its public link, with no session")
    quote = create("quotes", {
        "customer_id": customer, "issued_on": "2026-11-03", "valid_until": "2099-12-31",
        "lines": [{"product_id": product, "description": "Café", "quantity": "2", "unit_price": "10"}],
    })
    sent = owner.json("POST", f"{base}/quotes/{quote['id']}/send", {"to": "cliente@example.test"})
    token = sent["link"].rstrip("/").rsplit("/", 1)[1]
    stranger = Client(arguments.api)
    seen = stranger.json("GET", f"/public/quotes/{token}")
    check(seen["customer_name"] == "Cliente" and seen["status"] == "sent", "the customer sees the quote at the link")
    check(stranger.json("POST", f"/public/quotes/{token}/approve")["status"] == "approved", "the customer approves it")
    check(owner.json("GET", f"{base}/quotes/{quote['id']}")["status"] == "approved", "the company sees it approved")

    print("A charge and its payment page")
    owner.json("PUT", f"{base}/collection/settings", {"bank_account_id": account, "fee_category_id": None})
    entry = completed["receivable_id"]
    charge = owner.json("POST", f"{base}/entries/{entry}/charges", {"installment": 1, "method": "pix"}, expect=201)
    link = owner.json("POST", f"{base}/entries/{entry}/charges/{charge['id']}/link", expect=201)["url"]
    page = stranger.json("GET", f"/public/charges/{link.rstrip('/').rsplit('/', 1)[1]}")
    check(page["state"] == "payable", "the customer's page says the charge is payable")

    print("The reports and the dashboard's sources read")
    for path in (
        "/balances?on=2026-11-30", "/cash-flow?from=2026-11-01&to=2026-11-30&granularity=day", "/reports/overdue?on=2026-12-31",
        "/dre?from=2026-11-01&to=2026-11-30&by=month", "/sales-reports?by=product&from=2026-11-01&to=2026-11-30",
        "/delinquency?on=2026-12-31", "/stock-reports/valuation?on=2026-11-30", "/stock/movements", "/entries", "/sales", "/quotes",
        "/parties", "/commissions?from=2026-11-01&to=2026-11-30",
    ):
        status, _ = owner.call("GET", base + path)
        check(status == 200, f"GET {path.split('?')[0]} answers {status}")

    print("The password reset goes through the job queue")
    status, _ = Client(arguments.api).call("POST", "/auth/password-reset", {"email": f"dona-{tag}@example.test"})
    check(status == 202, "asking for a reset answers 202")

    print("A second company sees none of it")
    other = Client(arguments.api)
    other.json(
        "POST", "/auth/register",
        {"name": "Outro", "email": f"outro-{tag}@example.test", "password": password, "company_name": f"Loja {tag}"}, expect=201,
    )
    mine = other.json("GET", "/auth/me")["companies"][0]["id"]
    check(other.json("GET", f"/companies/{mine}/sales")["items"] == [], "its own sales are empty")
    check(other.call("GET", f"{base}/sales")[0] == 404, "the first company's sales are not reachable")
    check(other.json("GET", f"/companies/{mine}/parties")["items"] == [], "its parties are empty")
    print("e2e ok")


if __name__ == "__main__":
    main()
