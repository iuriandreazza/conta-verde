#!/usr/bin/env python3
"""End-to-end check of the first e-commerce connector, against a running API and a real database (ADR 0090).

A company turns the WooCommerce connector on from its own screens' API, and the connector package (connectors/woocommerce,
Node 22) runs against the **real** API with a **stand-in** for the store: a paid order becomes an open sale with its namespaced
reference (once, however many times it is read), completing the sale in conta-verde completes the order in the store through a
signed webhook sent by the real worker, a cancelled order cancels its open sale, and turning the connector off stops the key.

    docker compose up -d db
    export $(cat .env.example | xargs) CNPJ_LOOKUP=off BIND_ADDR=127.0.0.1:3100
    export FISCAL_SECRETS_KEY=$(python3 -c 'print("11" * 32)') WEBHOOK_ALLOW_PRIVATE_TARGETS=true
    cargo run -p conta-verde-api &
    uv run scripts/e2e-woocommerce.py http://127.0.0.1:3100

WEBHOOK_ALLOW_PRIVATE_TARGETS lets the API send the webhook to the connector on this machine (development only). It makes its own
accounts (random addresses and passwords that are never printed) and leaves them in the database. The store is a stand-in: this
does not prove anything about a real WooCommerce.
"""

import argparse
import http.cookiejar
import json
import os
import secrets
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

PACKAGE = Path(__file__).resolve().parent.parent / "connectors" / "woocommerce"


class Client:
    def __init__(self, base):
        self.base = base.rstrip("/")
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def call(self, method, path, body=None, bearer=None):
        data = None if body is None else json.dumps(body).encode()
        request = urllib.request.Request(self.base + path, data=data, method=method)
        if data is not None:
            request.add_header("Content-Type", "application/json")
        if bearer:
            request.add_header("Authorization", f"Bearer {bearer}")
        try:
            with self.opener.open(request) as response:
                payload = response.read()
                return response.status, json.loads(payload) if payload else None
        except urllib.error.HTTPError as error:
            payload = error.read()
            return error.code, json.loads(payload) if payload else None

    def json(self, method, path, body=None, expect=200, bearer=None):
        status, payload = self.call(method, path, body, bearer)
        assert status == expect, f"{method} {path}: expected {expect}, got {status}: {payload}"
        return payload


def check(condition, message):
    if not condition:
        raise AssertionError(message)
    print(f"  ok  {message}")


def free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


class Store:
    """A stand-in for the store's REST API: orders, read by modified_after and updated by status."""

    def __init__(self):
        self.orders = []
        self.updates = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def reply(self, status, body):
                payload = json.dumps(body).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)

            def do_GET(self):
                path, _, query = self.path.partition("?")
                if path.endswith("/orders"):
                    params = dict(pair.split("=", 1) for pair in query.split("&") if "=" in pair)
                    page, per_page = int(params.get("page", 1)), int(params.get("per_page", 10))
                    since = urllib.request.unquote(params.get("modified_after", ""))
                    found = [o for o in outer.orders if o["date_modified_gmt"] > since]
                    return self.reply(200, found[(page - 1) * per_page : page * per_page])
                order = next((o for o in outer.orders if path.endswith(f"/orders/{o['id']}")), None)
                self.reply(200, order) if order else self.reply(404, {})

            def do_PUT(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                order = next((o for o in outer.orders if self.path.split("?")[0].endswith(f"/orders/{o['id']}")), None)
                if not order:
                    return self.reply(404, {})
                order["status"] = body["status"]
                outer.updates.append((order["id"], body["status"]))
                self.reply(200, order)

        self.port = free_port()
        self.server = ThreadingHTTPServer(("127.0.0.1", self.port), Handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def order(self, number, sku, quantity, subtotal, total, status="processing"):
        order = {
            "id": number, "number": str(number), "status": status, "currency": "BRL",
            "date_created": "2026-11-05T10:00:00", "date_modified_gmt": "2026-11-05T13:00:00",
            "line_items": [{"id": 1, "name": "Café 500g", "sku": sku, "quantity": quantity, "subtotal": subtotal, "total": total}],
        }
        self.orders.append(order)
        return order


def wait_for(condition, what, seconds=40):
    deadline = time.time() + seconds
    while time.time() < deadline:
        if condition():
            return
        time.sleep(0.3)
    raise AssertionError(f"timed out waiting for {what}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("api", help="the API's address, e.g. http://127.0.0.1:3100")
    arguments = parser.parse_args()

    tag = secrets.token_hex(4)
    password = secrets.token_urlsafe(18)
    owner = Client(arguments.api)
    owner.json("POST", "/auth/register", {"name": "Dona", "email": f"dona-{tag}@example.test", "password": password, "company_name": f"Loja {tag}"}, expect=201)
    company = owner.json("GET", "/auth/me")["companies"][0]["id"]
    base = f"/companies/{company}"
    create = lambda path, body: owner.json("POST", f"{base}/{path}", body, expect=201)["id"]

    print("The company registers what the store's sales need")
    category = create("categories", {"name": "Vendas", "dre_group": "gross_revenue", "cash_flow_group": "operating"})
    customer = create("parties", {"name": "Loja online", "document": "11.222.333/0001-81", "is_customer": True, "is_supplier": False})
    sku = f"CAFE-{tag}".upper()
    create("products", {"kind": "product", "sku": sku, "name": "Café 500g", "unit": "UN", "sale_price": "10.10"})

    print("It turns the connector on, at the address where the package will listen")
    port = free_port()
    created = owner.json("POST", f"{base}/connectors", {"kind": "woocommerce", "name": "Loja do centro"}, expect=201)
    turned = owner.json("POST", f"{base}/connectors/{created['id']}/activate", {"address": f"http://127.0.0.1:{port}/webhooks/conta-verde"})
    api_key, webhook_secret = turned["api_key_secret"], turned["webhook_secret"]
    check(turned["connector"]["state"] == "on", "the connector is on, with a key and a subscription")
    status, _ = owner.call("GET", "/v1/parties", bearer=api_key)
    check(status == 403, "its key cannot read what it does not need (parties)")

    print("The package runs against the real API, with the store stand-in")
    store = Store()
    store.order(9001, sku, 2, "20.20", "18.20")
    store.order(9002, sku, 1, "10.10", "10.10", status="pending")
    state_dir = tempfile.mkdtemp(prefix="connector-")
    env = {
        **os.environ, "STORE_ID": "centro", "CONTA_VERDE_URL": f"{arguments.api.rstrip('/')}/v1", "CONTA_VERDE_API_KEY": api_key,
        "CONTA_VERDE_CUSTOMER_ID": customer, "CONTA_VERDE_CATEGORY_ID": category, "WEBHOOK_SECRET": webhook_secret,
        "WOO_URL": f"http://127.0.0.1:{store.port}", "WOO_CONSUMER_KEY": "ck_x", "WOO_CONSUMER_SECRET": "cs_x",
        "PORT": str(port), "POLL_SECONDS": "2", "STATE_FILE": f"{state_dir}/state.json", "START_FROM": "2026-11-01T00:00:00Z",
    }
    process = subprocess.Popen(["node", "src/main.js"], cwd=PACKAGE, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    try:
        reference = "woocommerce:centro:9001"
        wait_for(lambda: owner.json("GET", f"/v1/sales?external_reference={reference}", bearer=api_key)["items"], "the order to become a sale")
        sale = owner.json("GET", f"/v1/sales?external_reference={reference}", bearer=api_key)["items"][0]
        check(sale["status"] == "open" and sale["total"] == "18.2", f"a paid order is an open sale worth the order's total ({sale['total']})")
        check(sale["lines"][0]["unit_price"] == "10.1" and sale["lines"][0]["discount"] == "2", "its line has the order's own price and the coupon as discount")
        time.sleep(5)
        sales = owner.json("GET", "/v1/sales", bearer=api_key)["items"]
        check(len([s for s in sales if s["external_reference"] == reference]) == 1, "read again on later passes, it is still one sale")
        check(not any(s["external_reference"] == "woocommerce:centro:9002" for s in sales), "an order that is not paid yet is not imported")

        print("Completing the sale in conta-verde completes the order in the store, through the signed webhook")
        owner.json("POST", f"/v1/sales/{sale['id']}/complete", {}, bearer=api_key)
        wait_for(lambda: (9001, "completed") in store.updates, "the webhook to complete the order")
        check(True, "the order was completed in the store by a delivery the connector verified")
        deliveries = owner.json("GET", f"{base}/webhook-subscriptions")["items"][0]["id"]
        log = owner.json("GET", f"{base}/webhook-subscriptions/{deliveries}/deliveries")["items"]
        check(any(d["kind"] == "sale.completed" and d["status"] == "delivered" for d in log), "the delivery log says delivered")

        print("An order taken back cancels its open sale")
        store.order(9003, sku, 1, "10.10", "10.10")
        wait_for(lambda: owner.json("GET", "/v1/sales?external_reference=woocommerce:centro:9003", bearer=api_key)["items"], "the second order")
        store.orders[-1].update(status="cancelled", date_modified_gmt="2026-11-06T09:00:00")
        wait_for(lambda: owner.json("GET", "/v1/sales?external_reference=woocommerce:centro:9003", bearer=api_key)["items"][0]["status"] == "cancelled", "the sale to be cancelled")
        check(True, "the sale of a cancelled order is cancelled")
    finally:
        process.terminate()
        process.wait(timeout=10)

    print("Turning the connector off revokes the key and switches the webhook off")
    owner.json("POST", f"{base}/connectors/{created['id']}/deactivate")
    status, _ = owner.call("GET", "/v1/products", bearer=api_key)
    check(status == 401, "the key no longer opens the API")
    check(owner.json("GET", f"{base}/webhook-subscriptions")["items"][0]["active"] is False, "the subscription is off")
    print("All checks passed.")


if __name__ == "__main__":
    sys.exit(main())
