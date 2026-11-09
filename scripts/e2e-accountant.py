#!/usr/bin/env python3
"""End-to-end check of the exit criterion of Phase 6, against a running API and a real database (ADR 0067, 0071).

An owner sets a company up, invites an accountant, and the accountant, from their own login, closes a month and
downloads everything: a locked month refuses changes, the accountant cannot write the books, and the files hold the
month's entries, settlements, transfers and attachments exactly.

    docker compose up -d db
    export $(cat .env.example | xargs) CNPJ_LOOKUP=off BIND_ADDR=127.0.0.1:3100
    cargo run -p conta-verde-api &
    uv run scripts/e2e-accountant.py http://127.0.0.1:3100

It makes its own accounts (random addresses, random passwords that are never printed) and leaves them in the database,
so run it against a development database. `--credentials-file PATH` writes the accountant's login there, for a look at
the same company in the web app.
"""

import argparse
import csv
import http.cookiejar
import io
import json
import secrets
import sys
import urllib.error
import urllib.parse
import urllib.request
import zipfile

SPAN = ("2026-10-01", "2026-10-31")
NOTE = b"%PDF-1.4 nota fiscal de teste"


class Client:
    """One person's session: a cookie jar and the JSON the API speaks."""

    def __init__(self, base):
        self.base = base.rstrip("/")
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def call(self, method, path, body=None, raw=None, content_type=None):
        data = raw if raw is not None else (None if body is None else json.dumps(body).encode())
        request = urllib.request.Request(self.base + path, data=data, method=method)
        if data is not None:
            request.add_header("Content-Type", content_type or "application/json")
        try:
            with self.opener.open(request) as response:
                return response.status, response.headers, response.read()
        except urllib.error.HTTPError as error:
            return error.code, error.headers, error.read()

    def json(self, method, path, body=None, expect=200):
        status, _, payload = self.call(method, path, body)
        assert status == expect, f"{method} {path}: expected {expect}, got {status}: {payload[:300]!r}"
        return json.loads(payload) if payload else None


def check(condition, message):
    if not condition:
        raise AssertionError(message)
    print(f"  ok  {message}")


def csv_rows(payload):
    return list(csv.reader(io.StringIO(payload.decode("utf-8-sig")), delimiter=";"))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("api", help="the API's address, e.g. http://127.0.0.1:3100")
    parser.add_argument("--credentials-file", help="where to write the accountant's e-mail and password")
    arguments = parser.parse_args()

    tag = secrets.token_hex(4)
    password = secrets.token_urlsafe(18)
    owner_email, accountant_email = f"dona-{tag}@example.test", f"contadora-{tag}@example.test"

    print("The owner sets the company up")
    owner = Client(arguments.api)
    owner.json(
        "POST",
        "/auth/register",
        {"name": "Dona", "email": owner_email, "password": password, "company_name": f"Padaria {tag}"},
        expect=201,
    )
    company = owner.json("GET", "/auth/me")["companies"][0]["id"]
    base = f"/companies/{company}"

    def create(path, body):
        return owner.json("POST", f"{base}/{path}", body, expect=201)["id"]

    revenue = create("categories", {"name": "Vendas", "dre_group": "gross_revenue", "cash_flow_group": "operating"})
    expenses = create("categories", {"name": "Aluguel", "dre_group": "operating_expenses", "cash_flow_group": "operating"})
    create("categories", {"name": "Transferências", "dre_group": "transfers", "cash_flow_group": "transfer"})
    cost_center = create("cost-centers", {"name": "Matriz"})
    party = create("parties", {"name": "Padaria Central", "document": "11.222.333/0001-81", "is_customer": True, "is_supplier": True})
    cash = create("bank-accounts", {"kind": "cash", "name": "Caixa"})
    bank = create("bank-accounts", {"kind": "cash", "name": "Banco"})

    def entry(description, kind, category, competence, parts):
        return {
            "kind": kind,
            "description": description,
            "party_id": party,
            "category_id": category,
            "cost_center_id": cost_center,
            "competence_date": competence,
            "installments": [{"due_date": due, "amount": amount} for due, amount in parts],
        }

    sale = create("entries", entry("Venda do mês", "receivable", revenue, "2026-10-05", [("2026-10-20", "600"), ("2026-11-20", "400")]))
    rent = create("entries", entry("Aluguel; outubro", "payable", expenses, "2026-10-31", [("2026-10-31", "200.5")]))
    september = create("entries", entry("Setembro", "receivable", revenue, "2026-09-30", [("2026-10-10", "10")]))
    create("entries", entry("Novembro", "receivable", revenue, "2026-11-01", [("2026-11-30", "70")]))
    owner.json(
        "POST", f"{base}/entries/{sale}/settlements",
        {"installment": 1, "settled_on": "2026-10-20", "applied": "600", "interest": "5", "bank_account_id": cash}, expect=201,
    )
    owner.json(
        "POST", f"{base}/entries/{rent}/settlements",
        {"installment": 1, "settled_on": "2026-10-31", "applied": "200.5", "bank_account_id": cash}, expect=201,
    )
    create("transfers", {"from_account_id": cash, "to_account_id": bank, "amount": "30.25", "occurred_on": "2026-10-10"})
    for target, name, content in ((sale, "Nota ç.pdf", NOTE), (september, "Antigo.pdf", b"%PDF-1.4 velho")):
        status, _, payload = owner.call(
            "POST", f"{base}/entries/{target}/attachments?name={urllib.parse.quote(name)}", raw=content, content_type="application/pdf"
        )
        assert status == 201, payload

    print("The owner invites the accountant, who signs up through the invitation")
    invited = owner.json("POST", f"{base}/invitations", {"email": accountant_email, "role": "accountant"}, expect=201)
    token = invited["link"].rstrip("/").rsplit("/", 1)[1]
    accountant = Client(arguments.api)
    accountant.json(
        "POST", "/auth/register",
        {"name": "Contadora", "email": accountant_email, "password": password, "invitation": token}, expect=201,
    )
    mine = next(c for c in accountant.json("GET", "/auth/me")["companies"] if c["id"] == company)
    check(mine["role"] == "accountant" and mine["permissions"].get("accounting") == "write", "the accountant writes only accounting")
    check(mine["permissions"].get("finance") == "read", "and reads the finance area")

    print("The accountant reads the books but cannot change them")
    status, _, _ = accountant.call("POST", f"{base}/entries", entry("Não pode", "receivable", revenue, "2026-12-01", [("2026-12-10", "1")]))
    check(status == 403, "a new entry is refused with 403")

    print("The accountant closes October, from their own login")
    accountant.json("PUT", f"{base}/accounting/period-lock", {"locked_through": "2026-10-31"})
    check(accountant.json("GET", f"{base}/accounting/period-lock")["locked_through"] == "2026-10-31", "the month is closed")
    status, _, payload = owner.call("POST", f"{base}/entries", entry("Tarde", "receivable", revenue, "2026-10-15", [("2026-11-15", "5")]))
    check(status == 409 and b"closed" in payload, "a closed month refuses a new entry (409)")
    status, _, payload = owner.call(
        "POST", f"{base}/entries/{sale}/settlements",
        {"installment": 2, "settled_on": "2026-10-31", "applied": "1", "bank_account_id": cash},
    )
    check(status == 409, "and a payment dated in it")
    owner.json("POST", f"{base}/entries", entry("Dezembro", "receivable", revenue, "2026-12-01", [("2026-12-10", "1")]), expect=201)
    print("  ok  the first day after the lock is still open")

    print("The accountant downloads everything for their software")
    exports = f"{base}/accounting/exports"

    def download(name, file_format="csv"):
        query = urllib.parse.urlencode({"from": SPAN[0], "to": SPAN[1], "format": file_format})
        status, headers, payload = accountant.call("GET", f"{exports}/{name}?{query}")
        assert status == 200, f"{name}: {status} {payload[:200]!r}"
        check("attachment" in headers["Content-Disposition"] and headers["X-Content-Type-Options"] == "nosniff", f"{name} is a download")
        return payload

    entries = csv_rows(download("entries"))
    check(entries[0][:4] == ["entry_id", "kind", "description", "party"], "the entries have the documented columns")
    check([(r[0], r[8], r[11]) for r in entries[1:]] == [(sale, "1", "600"), (sale, "2", "400"), (rent, "1", "200.5")],
          "the entries are the month's installments, nothing of September or November")
    settlements = csv_rows(download("settlements"))
    check([(r[1], r[11], r[12], r[15]) for r in settlements[1:]] == [(sale, "600", "5", "605"), (rent, "200.5", "0", "200.5")],
          "the settlements are the month's, with the cash worked out")
    transfers = csv_rows(download("transfers"))
    check([(r[2], r[3], r[4]) for r in transfers[1:]] == [("Caixa", "Banco", "30.25")], "the transfer is there")

    package = zipfile.ZipFile(io.BytesIO(download("package")))
    check(package.testzip() is None, "the package is a valid zip")
    names = package.namelist()
    kept = [n for n in names if n.startswith("attachments/") and n != "attachments.csv"]
    check(names[:4] == ["entries.csv", "settlements.csv", "transfers.csv", "attachments.csv"] and len(kept) == 1,
          "the package has the three files, the list and one attached file")
    check(package.read(kept[0]) == NOTE, "the attached file is byte for byte what was attached")
    check(package.read("entries.csv") == download("entries"), "the entries in the package are the ones downloaded alone")
    sheet = zipfile.ZipFile(io.BytesIO(download("entries", "xlsx")))
    check(sheet.testzip() is None and "xl/worksheets/sheet1.xml" in sheet.namelist(), "the spreadsheet is a valid xlsx container")

    print("The accountant reopens, with a reason")
    accountant.json("PUT", f"{base}/accounting/period-lock", {"locked_through": "2026-09-30", "reason": "nota chegou atrasada"})
    owner.json("POST", f"{base}/entries", entry("Agora cabe", "receivable", revenue, "2026-10-15", [("2026-11-15", "5")]), expect=201)
    print("  ok  October takes a new entry again")

    if arguments.credentials_file:
        with open(arguments.credentials_file, "w", encoding="utf-8") as out:
            json.dump({"email": accountant_email, "password": password, "company": company}, out)
    print("\ne2e ok")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as failure:
        print(f"\nFAILED: {failure}", file=sys.stderr)
        sys.exit(1)
