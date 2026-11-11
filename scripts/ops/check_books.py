#!/usr/bin/env python3
"""Puts known books in a running server and, later, checks that they are still there (ADR 0096).

    python3 scripts/ops/check_books.py seed   http://127.0.0.1:3100 state.json
    ... a backup, a restore or an upgrade ...
    python3 scripts/ops/check_books.py verify http://127.0.0.1:3100 state.json

`seed` makes a user (a random password, kept only in the state file), a company, a customer, a product, a bank account, two
receivables of which one is settled, a stock movement and a file attached to an entry, and writes what it made and what the books
say about it to the state file. `verify` signs in again with that user and checks every one of them, the balance and the bytes of
the file. It uses only the stable parts of the API of the SPA (registers, entries, balances, attachments), so it can run against a
server one release older than the checker. Exit status 1 when something is missing or different.
"""

import hashlib
import http.cookiejar
import json
import secrets
import sys
import urllib.error
import urllib.parse
import urllib.request

ON = "2026-12-31"


class Client:
    def __init__(self, base):
        self.base = base.rstrip("/")
        # A server behind TLS sets a Secure cookie (COOKIE_SECURE=true); a check run on the machine itself over http must still send it.
        policy = http.cookiejar.DefaultCookiePolicy(secure_protocols=("https", "http"))
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar(policy)))

    def call(self, method, path, body=None, raw=None, content_type="application/json"):
        data = raw if raw is not None else (None if body is None else json.dumps(body).encode())
        request = urllib.request.Request(self.base + path, data=data, method=method)
        if data is not None:
            request.add_header("Content-Type", content_type)
        try:
            with self.opener.open(request) as response:
                return response.status, response.read()
        except urllib.error.HTTPError as error:
            return error.code, error.read()

    def json(self, method, path, body=None, expect=200):
        status, payload = self.call(method, path, body)
        assert status == expect, f"{method} {path}: expected {expect}, got {status}: {payload[:200]!r}"
        return json.loads(payload) if payload else None


def check(condition, message):
    if not condition:
        print(f"FAILED: {message}")
        sys.exit(1)
    print(f"  ok  {message}")


def seed(api, state_file):
    tag = secrets.token_hex(4)
    password = secrets.token_urlsafe(18)
    email = f"check-{tag}@example.test"
    client = Client(api)
    client.json("POST", "/auth/register", {"name": "Conferência", "email": email, "password": password, "company_name": f"Empresa {tag}"}, expect=201)
    company = client.json("GET", "/auth/me")["companies"][0]["id"]
    base = f"/companies/{company}"

    def create(path, body):
        return client.json("POST", f"{base}/{path}", body, expect=201)["id"]

    revenue = create("categories", {"name": "Vendas", "dre_group": "gross_revenue", "cash_flow_group": "operating"})
    create("categories", {"name": "Transferências", "dre_group": "transfers", "cash_flow_group": "transfer"})
    customer = create("parties", {"name": "Cliente da conferência", "is_customer": True, "is_supplier": False})
    product = create("products", {"kind": "product", "name": "Café", "sku": f"CAFE-{tag}".upper(), "unit": "un", "active": True})
    account = create("bank-accounts", {"kind": "cash", "name": "Caixa"})
    entries = []
    for number, due in enumerate(["2026-11-20", "2026-12-20"], start=1):
        entries.append(create("entries", {
            "kind": "receivable", "description": f"Venda {number}", "party_id": customer, "category_id": revenue,
            "competence_date": "2026-11-05", "installments": [{"due_date": due, "amount": "100.50"}],
        }))
    client.json("POST", f"{base}/entries/{entries[0]}/settlements",
                {"installment": 1, "settled_on": "2026-11-20", "applied": "100.50", "bank_account_id": account}, expect=201)
    client.json("POST", f"{base}/stock/movements",
                {"product_id": product, "kind": "in", "quantity": "7", "unit_cost": "3.5", "occurred_on": "2026-11-02"}, expect=201)
    content = secrets.token_bytes(4096)
    status, payload = client.call("POST", f"{base}/entries/{entries[1]}/attachments?name={urllib.parse.quote('nota.pdf')}",
                                  raw=content, content_type="application/pdf")
    assert status == 201, payload
    attachment = json.loads(payload)["id"]
    balance = client.json("GET", f"{base}/balances?on={ON}")
    state = {
        "email": email, "password": password, "company": company, "entries": entries, "customer": customer,
        "product": product, "attachment": attachment, "attachment_sha256": hashlib.sha256(content).hexdigest(),
        "balances": balance,
    }
    with open(state_file, "w") as handle:
        json.dump(state, handle)
    print(f"seeded company {company}: 2 entries (one settled), a stock movement and a 4 KiB attachment")


def verify(api, state_file):
    with open(state_file) as handle:
        state = json.load(handle)
    client = Client(api)
    client.json("POST", "/auth/login", {"email": state["email"], "password": state["password"]})
    check(True, "the user signs in with the password they had")
    company = state["company"]
    base = f"/companies/{company}"
    check(any(c["id"] == company for c in client.json("GET", "/auth/me")["companies"]), "the company is theirs")
    listed = {e["id"] for e in client.json("GET", f"{base}/entries")["items"]}
    check(set(state["entries"]) <= listed, "both entries are listed")
    parties = {p["id"] for p in client.json("GET", f"{base}/parties")["items"]}
    check(state["customer"] in parties, "the customer is there")
    positions = {p["product_id"]: p for p in client.json("GET", f"{base}/stock/positions")["items"]}
    check(positions.get(state["product"], {}).get("quantity") == "7", "the stock still holds 7")
    check(client.json("GET", f"{base}/balances?on={ON}") == state["balances"], "the balances are what they were")
    status, payload = client.call("GET", f"{base}/entries/{state['entries'][1]}/attachments/{state['attachment']}")
    check(status == 200 and hashlib.sha256(payload).hexdigest() == state["attachment_sha256"], "the attached file is byte for byte the same")
    print("books ok")


if __name__ == "__main__":
    if len(sys.argv) != 4 or sys.argv[1] not in ("seed", "verify"):
        sys.exit(__doc__)
    {"seed": seed, "verify": verify}[sys.argv[1]](sys.argv[2], sys.argv[3])
