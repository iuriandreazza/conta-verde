#!/usr/bin/env python3
"""Reads a company books from the API of the cloud ERP being left into local JSON files (ADR 0077).

    uv run scripts/erp-api-reader.py --env-file .env login
    uv run scripts/erp-api-reader.py --env-file .env login --callback "https://...?code=...&state=..."
    uv run scripts/erp-api-reader.py --env-file .env fetch --from 2020-01-01 --to 2026-12-31 --out-dir out/erp

`ERP_CLIENT_ID`, `ERP_CLIENT_SECRET` and optionally `ERP_OAUTH_URL` (the authorization address the app was registered
with) come from the environment or an `--env-file`. `login` prints the authorization address; when the app's redirect
is not a loopback address, the person authorizes in the browser and gives the address it lands on (or the bare code) to
`login --callback`, which exchanges it for tokens. Without `ERP_OAUTH_URL` it receives the code on a loopback address.
The tokens are kept in
`~/.config/conta-verde/erp-api-token.json` (mode 0600, outside the repository). `fetch` refreshes the token
when it must and writes one JSON file per register (categories, cost centers, financial accounts, people,
products, receivables, payables), exactly as the API answered. Nothing is written to conta-verde here: the
files are the input of the converter, which is checked by hand before anything is imported.

The client secret comes from the environment, never from an argument (it would reach the shell history).
Standard library only. The paths and shapes are those of the vendor's published OpenAPI files.
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

AUTH_URL = "https://api-v2.contaazul.com/oauth/authorize"
TOKEN_URL = "https://api-v2.contaazul.com/oauth/token"
API_URL = "https://api-v2.contaazul.com"
SCOPE = "openid profile aws.cognito.signin.user.admin"
PERSON_KINDS = ("Física", "Jurídica", "Estrangeira")
PERSON_PROFILES = ("Cliente", "Fornecedor", "Transportadora")
PRODUCT_STATUSES = ("ATIVO", "INATIVO")
LOOPBACK_HOSTS = ("localhost", "127.0.0.1")
DEFAULT_REDIRECT = "http://localhost:8765/callback"
TOKEN_FILE = Path.home() / ".config" / "conta-verde" / "erp-api-token.json"
PAGE_SIZE = 100
# The two payable/receivable listings need a due-date window; one window never spans more than this.
WINDOW_DAYS = 90
# The transfers and opening balances refuse a span of more than a year.
YEAR_WINDOW_DAYS = 365
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


def load_env_file(path: Path) -> None:
    """Sets the variables of a KEY=VALUE file that the environment does not already hold."""
    for line in path.read_text().splitlines():
        key, separator, value = line.partition("=")
        key = key.strip()
        if separator and key and not key.startswith("#"):
            os.environ.setdefault(key, value.strip().strip("\"'"))


def authorization_parameters(oauth_url: str) -> dict:
    """The query of the registered authorization address, which may sit behind a `#/` route."""
    return {key: values[0] for key, values in urllib.parse.parse_qs(oauth_url.partition("?")[2]).items()}


def code_from_callback(callback: str, expected_state: str | None) -> str:
    """The code of the address the browser landed on, or the bare code pasted instead."""
    if "code=" not in callback:
        return callback.strip()
    query = urllib.parse.parse_qs(urllib.parse.urlparse(callback.strip()).query)
    if "code" not in query:
        raise ErpApiError("the address has no code: authorize again and copy the address after the redirect")
    if expected_state and query.get("state", [expected_state])[0] != expected_state:
        raise ErpApiError("the answer did not carry the state we sent: authorization refused")
    return query["code"][0]


def login_with_callback(oauth_url: str, callback: str | None) -> None:
    parameters = authorization_parameters(oauth_url)
    if callback is None:
        print(f"Open this address in the browser and authorize the app:\n\n{oauth_url}\n", file=sys.stderr)
        print("Then run `login --callback` with the address the browser lands on (the code is short-lived).", file=sys.stderr)
        return
    code = code_from_callback(callback, parameters.get("state"))
    save_tokens(token_request({"grant_type": "authorization_code", "code": code, "redirect_uri": parameters["redirect_uri"]}))
    print(f"Tokens saved in {TOKEN_FILE}", file=sys.stderr)


def login(redirect_uri: str, authorize_url: str = AUTH_URL) -> None:
    client_id, _ = client_credentials()
    state = secrets.token_urlsafe(24)
    url = authorize_url + "?" + urllib.parse.urlencode(
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
            except (urllib.error.URLError, TimeoutError, ConnectionError):
                # A dropped connection is the network's, not an answer: wait and ask again like a 5xx.
                self.sleep(min(2**attempt, 30))
        raise ErpApiError(f"{path} kept failing after {MAX_ATTEMPTS} attempts")

    def pages(self, path: str, params: dict | None = None, page_size: int = PAGE_SIZE) -> list:
        """Every item of a paged listing: the answer is a list or an object whose `itens` (`items` in the cost centers' listing) is one."""
        items: list = []
        page = 1
        while True:
            answer = self.get(path, {**(params or {}), "pagina": page, "tamanho_pagina": page_size})
            batch = (answer.get("itens") or answer.get("items") or []) if isinstance(answer, dict) else answer
            items.extend(batch)
            if len(batch) < page_size:
                return items
            page += 1


def all_people(client: Client) -> list:
    """The listing defaults to individuals who are customers, so every kind and profile is asked for.

    A person with two profiles comes back under each, so the answers are joined by id."""
    people: dict = {}
    for kind in PERSON_KINDS:
        for profile in PERSON_PROFILES:
            for person in client.pages("/v1/pessoas", {"tipos_pessoa": kind, "tipo_perfil": profile, "com_endereco": "true"}):
                people[person["id"]] = person
    return list(people.values())


def windows(start: date, end: date, days: int = WINDOW_DAYS):
    """Consecutive spans of at most `days` days covering start..end, both included, none overlapping."""
    while start <= end:
        last = min(start + timedelta(days=days - 1), end)
        yield start, last
        start = last + timedelta(days=1)


def by_window(
    client: Client, path: str, start: date, end: date, first_key: str, last_key: str, days: int, with_time: bool = False
) -> list:
    """Reads a listing span by span; `with_time` is for the endpoints that want a date-time, from the first second to the last."""
    items: list = []
    for first, last in windows(start, end, days):
        first_value, last_value = first.isoformat(), last.isoformat()
        if with_time:
            first_value, last_value = first_value + "T00:00:00", last_value + "T23:59:59"
        items.extend(client.pages(path, {first_key: first_value, last_key: last_value}))
    return items


def by_due_date(client: Client, path: str, start: date, end: date) -> list:
    return by_window(client, path, start, end, "data_vencimento_de", "data_vencimento_ate", WINDOW_DAYS)


def fetch(client: Client, start: date, end: date, out_dir: Path, reuse: bool = False) -> dict[str, int]:
    out_dir.mkdir(parents=True, exist_ok=True)
    registers = {
        "categories-income": lambda: client.pages("/v1/categorias", {"tipo": "RECEITA", "permite_apenas_filhos": "false", "apenas_filhos": "false"}),
        "categories-expense": lambda: client.pages("/v1/categorias", {"tipo": "DESPESA", "permite_apenas_filhos": "false", "apenas_filhos": "false"}),
        "cost-centers": lambda: client.pages("/v1/centro-de-custo", {"filtro_rapido": "TODOS"}),
        "financial-accounts": lambda: client.pages("/v1/conta-financeira", {"apenas_ativo": "false"}),
        "people": lambda: all_people(client),
        "products": lambda: [
            item for status in PRODUCT_STATUSES for item in client.pages("/v1/produtos", {"status": status})
        ],
        "receivables": lambda: by_due_date(
            client, "/v1/financeiro/eventos-financeiros/contas-a-receber/buscar", start, end
        ),
        "payables": lambda: by_due_date(client, "/v1/financeiro/eventos-financeiros/contas-a-pagar/buscar", start, end),
        "transfers": lambda: by_window(
            client, "/v1/financeiro/transferencias", start, end, "data_inicio", "data_fim", YEAR_WINDOW_DAYS
        ),
        "opening-balances": lambda: by_window(
            client, "/v1/financeiro/eventos-financeiros/saldo-inicial", start, end, "data_inicio", "data_fim", YEAR_WINDOW_DAYS, with_time=True
        ),
    }
    counts = {}
    for name, read in registers.items():
        target = out_dir / f"{name}.json"
        # Reusing is for finishing a run that failed halfway; a fresh run reads everything again.
        items = json.loads(target.read_text()) if reuse and target.exists() else read()
        target.write_text(json.dumps(items, ensure_ascii=False, indent=1))
        counts[name] = len(items)
        print(f"{name}: {len(items)}", file=sys.stderr)
    installments = [item["id"] for name in ("receivables", "payables") for item in json.loads((out_dir / f"{name}.json").read_text())]
    counts["installment-details"] = fetch_installment_details(client, installments, out_dir / "installments")
    return counts


def fetch_installment_details(client: Client, ids: list[str], directory: Path) -> int:
    """One file per installment: the listing carries neither the split by category and cost center nor the payments.

    A file already there is kept, so an interrupted run goes on where it stopped."""
    directory.mkdir(parents=True, exist_ok=True)
    for number, installment_id in enumerate(ids, 1):
        target = directory / f"{installment_id}.json"
        if not target.exists():
            target.write_text(json.dumps(client.get(f"/v1/financeiro/eventos-financeiros/parcelas/{installment_id}", {}), ensure_ascii=False))
        if number % 100 == 0:
            print(f"installment-details: {number}/{len(ids)}", file=sys.stderr)
    return len(ids)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--env-file", type=Path, help="KEY=VALUE file with the ERP_* variables")
    commands = parser.add_subparsers(dest="command", required=True)
    login_parser = commands.add_parser("login", help="authorize the app and keep the tokens")
    login_parser.add_argument("--redirect-uri", default=DEFAULT_REDIRECT, help="loopback mode: must be registered in the app")
    login_parser.add_argument("--callback", help="the address the browser landed on after authorizing, or the bare code")
    fetch_parser = commands.add_parser("fetch", help="write one JSON file per register")
    fetch_parser.add_argument("--from", dest="start", type=date.fromisoformat, required=True)
    fetch_parser.add_argument("--to", dest="end", type=date.fromisoformat, required=True)
    fetch_parser.add_argument("--out-dir", type=Path, required=True)
    fetch_parser.add_argument("--reuse", action="store_true", help="keep the registers already written in --out-dir")
    args = parser.parse_args(argv)
    try:
        if args.env_file:
            load_env_file(args.env_file)
        if args.command == "login":
            oauth_url = os.environ.get("ERP_OAUTH_URL")
            redirect = authorization_parameters(oauth_url or "").get("redirect_uri", args.redirect_uri)
            if oauth_url and urllib.parse.urlparse(redirect).hostname in LOOPBACK_HOSTS:
                login(redirect, oauth_url.partition("?")[0])
            elif oauth_url:
                login_with_callback(oauth_url, args.callback)
            else:
                login(args.redirect_uri)
        else:
            fetch(Client(), args.start, args.end, args.out_dir, args.reuse)
    except ErpApiError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
