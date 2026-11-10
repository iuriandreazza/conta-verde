#!/usr/bin/env python3
"""Reads a company books from the API of the cloud ERP being left into local JSON files (ADR 0077).

    export ERP_CLIENT_ID=... ERP_CLIENT_SECRET=...
    uv run scripts/erp-api-reader.py login
    uv run scripts/erp-api-reader.py fetch --from 2020-01-01 --to 2026-12-31 --out-dir out/erp

`login` opens the authorization page, receives the code on a loopback address and keeps the tokens in
`~/.config/conta-verde/erp-api-token.json` (mode 0600, outside the repository). `fetch` refreshes the token
when it must and writes one JSON file per register (categories, cost centers, financial accounts, people,
products, receivables, payables), exactly as the API answered. Nothing is written to conta-verde here: the
files are the input of the converter, which is checked by hand before anything is imported.

The client secret comes from the environment, never from an argument (it would reach the shell history).
Standard library only.
"""

import argparse
import base64
import json
import os
import secrets
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import date, timedelta
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

AUTH_URL = "https://auth.contaazul.com/oauth2/authorize"
TOKEN_URL = "https://auth.contaazul.com/oauth2/token"
API_URL = "https://api-v2.contaazul.com"
SCOPE = "openid profile aws.cognito.signin.user.admin"
DEFAULT_REDIRECT = "http://localhost:8765/callback"
TOKEN_FILE = Path.home() / ".config" / "conta-verde" / "erp-api-token.json"
PAGE_SIZE = 100
# The two payable/receivable listings need a due-date window; one window never spans more than this.
WINDOW_DAYS = 90
MAX_ATTEMPTS = 5
# A token this close to its end is renewed first, so a long page loop never meets a 401 halfway.
EXPIRY_MARGIN_SECONDS = 60


class ErpApiError(Exception):
    """What went wrong, in words for the person running the script."""


def client_credentials() -> tuple[str, str]:
    client_id = os.environ.get("ERP_CLIENT_ID", "")
    client_secret = os.environ.get("ERP_CLIENT_SECRET", "")
    if not client_id or not client_secret:
        raise ErpApiError("set ERP_CLIENT_ID and ERP_CLIENT_SECRET (the app's credentials)")
    return client_id, client_secret


def token_request(form: dict, token_url: str = TOKEN_URL) -> dict:
    client_id, client_secret = client_credentials()
    basic = base64.b64encode(f"{client_id}:{client_secret}".encode()).decode()
    request = urllib.request.Request(
        token_url,
        data=urllib.parse.urlencode(form).encode(),
        headers={"Authorization": f"Basic {basic}", "Content-Type": "application/x-www-form-urlencoded"},
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            body = json.load(response)
    except urllib.error.HTTPError as error:
        # The body can name the reason (invalid_grant, invalid_client); it never carries our secret.
        raise ErpApiError(f"the token endpoint answered {error.code}: {error.read()[:300].decode(errors='replace')}")
    return {
        "access_token": body["access_token"],
        "refresh_token": body.get("refresh_token"),
        "expires_at": time.time() + int(body.get("expires_in", 3600)),
    }


def save_tokens(tokens: dict, path: Path = TOKEN_FILE) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w") as handle:
        json.dump(tokens, handle)


def load_tokens(path: Path = TOKEN_FILE) -> dict:
    try:
        return json.loads(path.read_text())
    except FileNotFoundError:
        raise ErpApiError("no token yet: run `login` first")


def fresh_access_token(path: Path = TOKEN_FILE, token_url: str = TOKEN_URL, force: bool = False) -> str:
    tokens = load_tokens(path)
    if not force and tokens["expires_at"] - EXPIRY_MARGIN_SECONDS > time.time():
        return tokens["access_token"]
    if not tokens.get("refresh_token"):
        raise ErpApiError("the token expired and there is no refresh token: run `login` again")
    renewed = token_request({"grant_type": "refresh_token", "refresh_token": tokens["refresh_token"]}, token_url)
    # A refresh answer may omit the refresh token: the old one stays valid then.
    renewed["refresh_token"] = renewed["refresh_token"] or tokens["refresh_token"]
    save_tokens(renewed, path)
    return renewed["access_token"]


def wait_for_code(redirect_uri: str, state: str) -> str:
    target = urllib.parse.urlparse(redirect_uri)
    received: dict = {}

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            query = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
            if urllib.parse.urlparse(self.path).path != target.path:
                self.send_response(404)
                self.end_headers()
                return
            received.update({key: values[0] for key, values in query.items()})
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.end_headers()
            self.wfile.write("Authorized. You can close this tab.".encode())

        def log_message(self, *_):
            pass

    server = HTTPServer((target.hostname or "localhost", target.port or 80), Handler)
    while "code" not in received and "error" not in received:
        server.handle_request()
    server.server_close()
    if received.get("state") != state:
        raise ErpApiError("the answer did not carry the state we sent: authorization refused")
    if "error" in received:
        raise ErpApiError(f"authorization refused: {received['error']}")
    return received["code"]


def login(redirect_uri: str) -> None:
    client_id, _ = client_credentials()
    state = secrets.token_urlsafe(24)
    url = AUTH_URL + "?" + urllib.parse.urlencode(
        {"response_type": "code", "client_id": client_id, "redirect_uri": redirect_uri, "state": state, "scope": SCOPE}
    )
    print(f"Open this address in the browser and authorize the app:\n\n{url}\n", file=sys.stderr)
    code = wait_for_code(redirect_uri, state)
    save_tokens(token_request({"grant_type": "authorization_code", "code": code, "redirect_uri": redirect_uri}))
    print(f"Tokens saved in {TOKEN_FILE}", file=sys.stderr)


class Client:
    """Reads pages from the API, renewing the token once on a 401 and waiting out 429 and 5xx."""

    def __init__(self, api_url: str = API_URL, token_path: Path = TOKEN_FILE, token_url: str = TOKEN_URL, sleep=time.sleep):
        self.api_url = api_url
        self.token_path = token_path
        self.token_url = token_url
        self.sleep = sleep

    def get(self, path: str, params: dict) -> object:
        query = urllib.parse.urlencode(params, doseq=True)
        renewed = False
        for attempt in range(MAX_ATTEMPTS):
            token = fresh_access_token(self.token_path, self.token_url, force=renewed)
            request = urllib.request.Request(
                f"{self.api_url}{path}?{query}", headers={"Authorization": f"Bearer {token}", "Accept": "application/json"}
            )
            try:
                with urllib.request.urlopen(request, timeout=60) as response:
                    return json.load(response)
            except urllib.error.HTTPError as error:
                with error:
                    if error.code == 401 and not renewed:
                        renewed = True
                        continue
                    if error.code == 429 or error.code >= 500:
                        self.sleep(min(2**attempt, 30))
                        continue
                    raise ErpApiError(f"{path} answered {error.code}: {error.read()[:300].decode(errors='replace')}")
        raise ErpApiError(f"{path} kept failing after {MAX_ATTEMPTS} attempts")

    def pages(self, path: str, params: dict | None = None, page_size: int = PAGE_SIZE) -> list:
        """Every item of a paged listing: the answer is a list or an object whose `itens` is one."""
        items: list = []
        page = 1
        while True:
            answer = self.get(path, {**(params or {}), "pagina": page, "tamanho_pagina": page_size})
            batch = answer["itens"] if isinstance(answer, dict) else answer
            items.extend(batch)
            if len(batch) < page_size:
                return items
            page += 1


def windows(start: date, end: date, days: int = WINDOW_DAYS):
    """Consecutive spans of at most `days` days covering start..end, both included, none overlapping."""
    while start <= end:
        last = min(start + timedelta(days=days - 1), end)
        yield start, last
        start = last + timedelta(days=1)


def by_due_date(client: Client, path: str, start: date, end: date) -> list:
    items: list = []
    for first, last in windows(start, end):
        items.extend(
            client.pages(path, {"data_vencimento_de": first.isoformat(), "data_vencimento_ate": last.isoformat()})
        )
    return items


def fetch(client: Client, start: date, end: date, out_dir: Path) -> dict[str, int]:
    out_dir.mkdir(parents=True, exist_ok=True)
    registers = {
        "categories-income": lambda: client.pages("/v1/categorias", {"tipo": "RECEITA"}),
        "categories-expense": lambda: client.pages("/v1/categorias", {"tipo": "DESPESA"}),
        "cost-centers": lambda: client.pages("/v1/centro-de-custo", {"filtro_rapido": "TODOS"}),
        "financial-accounts": lambda: client.pages("/v1/conta-financeira", {"apenas_ativo": "false"}),
        "people": lambda: client.pages("/v1/pessoa"),
        "products": lambda: client.pages("/v1/produto/busca"),
        "receivables": lambda: by_due_date(
            client, "/v1/financeiro/eventos-financeiros/contas-a-receber/buscar", start, end
        ),
        "payables": lambda: by_due_date(client, "/v1/financeiro/eventos-financeiros/contas-a-pagar/buscar", start, end),
    }
    counts = {}
    for name, read in registers.items():
        items = read()
        (out_dir / f"{name}.json").write_text(json.dumps(items, ensure_ascii=False, indent=1))
        counts[name] = len(items)
        print(f"{name}: {len(items)}", file=sys.stderr)
    return counts


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    login_parser = commands.add_parser("login", help="authorize the app and keep the tokens")
    login_parser.add_argument("--redirect-uri", default=DEFAULT_REDIRECT, help="must be registered in the app")
    fetch_parser = commands.add_parser("fetch", help="write one JSON file per register")
    fetch_parser.add_argument("--from", dest="start", type=date.fromisoformat, required=True)
    fetch_parser.add_argument("--to", dest="end", type=date.fromisoformat, required=True)
    fetch_parser.add_argument("--out-dir", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "login":
            login(args.redirect_uri)
        else:
            fetch(Client(), args.start, args.end, args.out_dir)
    except ErpApiError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
