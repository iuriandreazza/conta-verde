#!/usr/bin/env python3
"""End-to-end check of the offline mode in a real Chromium, against a production build of the web app (ADR 0091).

It signs up, turns the offline mode on, opens a list, goes offline and checks what the worker did: the app opens, the cached list shows
with the banner that says it is the device's copy, what is not on the allowlist is not served, back online the label goes away, and
signing out leaves no data cache behind.

    docker compose up -d db
    export $(cat .env.example | xargs) CNPJ_LOOKUP=off BIND_ADDR=127.0.0.1:3111
    cargo run -p conta-verde-api &
    pnpm --filter conta-verde-web build
    VITE_API_TARGET=http://127.0.0.1:3111 pnpm --filter conta-verde-web exec vite preview --port 5266 &
    uv run --with playwright python scripts/e2e-pwa.py http://localhost:5266

Playwright needs its Chromium once (`uv run --with playwright python -m playwright install chromium`); CHROMIUM=/path uses another.
It makes its own account (a random address and password that is never printed) and leaves it in the database.
"""

import os
import secrets
import sys

from playwright.sync_api import sync_playwright


def check(ok, message):
    if not ok:
        raise AssertionError(message)
    print(f"  ok  {message}")


def main():
    base = (sys.argv[1] if len(sys.argv) > 1 else "http://localhost:5266").rstrip("/")
    tag, password = secrets.token_hex(4), secrets.token_urlsafe(18)
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=os.environ.get("CHROMIUM") or None, headless=True)
        context = browser.new_context()
        page = context.new_page()
        page.goto(base + "/login")
        status = page.evaluate(
            """async ([email, password]) => (await fetch('/api/auth/register', {method: 'POST', headers: {'content-type': 'application/json'},
            body: JSON.stringify({name: 'Teste', email, password, company_name: 'Loja Offline'})})).status""",
            [f"pwa-{tag}@example.test", password],
        )
        check(status == 201, "signed up")
        company = page.evaluate("fetch('/api/auth/me').then(r => r.json())")["companies"][0]["id"]
        page.evaluate(
            """async (company) => { for (const name of ['Maria Silva', 'Joao Souza']) await fetch(`/api/companies/${company}/parties`,
            {method: 'POST', headers: {'content-type': 'application/json'}, body: JSON.stringify({name, is_customer: true, is_supplier: false})}) }""",
            company,
        )

        page.goto(base + "/offline")
        page.wait_for_selector("text=Uso offline")
        page.wait_for_function("navigator.serviceWorker.getRegistration().then(r => !!r && !!r.active)", timeout=15000)
        check(True, "the service worker is registered and active")
        page.click("text=Ligar o modo offline neste aparelho")
        page.wait_for_selector("text=Desligar e apagar a cópia")
        page.reload()
        page.wait_for_function("!!navigator.serviceWorker.controller", timeout=10000)
        check(True, "the page is controlled by the worker")

        page.goto(f"{base}/companies/{company}/parties")
        page.wait_for_selector("text=Maria Silva")
        page.goto(f"{base}/companies/{company}/members")
        page.wait_for_timeout(500)
        names = page.evaluate("caches.keys()")
        check(any(n.startswith("data:") and n.endswith(company) for n in names), f"the list was cached for this person and company: {names}")
        cached = page.evaluate(
            """async () => { const out = []; for (const name of await caches.keys()) { const c = await caches.open(name);
            for (const r of await c.keys()) out.push(new URL(r.url).pathname); } return out }"""
        )
        check(not any("members" in u or ("/auth" in u and not u.endswith("/auth/me")) for u in cached), f"nothing outside the allowlist was cached: {cached}")

        print("Offline")
        context.set_offline(True)
        page.goto(f"{base}/companies/{company}/parties")
        page.wait_for_selector("text=Maria Silva", timeout=10000)
        check(True, "the app opens and the cached list shows with no network")
        page.wait_for_selector("text=uma cópia guardada neste aparelho")
        check(True, "the banner says it is the device's copy and that nothing can be saved")
        status = page.evaluate(f"fetch('/api/companies/{company}/members').then(r => r.status).catch(() => 'network-error')")
        check(status == "network-error", "what is not allowlisted is not served offline (members)")
        context.set_offline(False)
        page.wait_for_selector("text=A conexão voltou")
        check(True, "back online, the screen still says part of it is the copy, and offers to refresh")
        page.click("button:has-text('Atualizar')")
        page.wait_for_selector("text=Maria Silva")
        page.wait_for_function("!document.body.innerText.includes('cópia guardada')", timeout=10000)
        check(True, "after refreshing, the label goes away")

        print("Sign out clears the copy")
        page.click("button:has-text('Sair')")
        page.wait_for_url("**/login*", timeout=10000)
        check([n for n in page.evaluate("caches.keys()") if n.startswith("data:")] == [], "no data cache is left after signing out")
        browser.close()
    print("All checks passed.")


if __name__ == "__main__":
    main()
