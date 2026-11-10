"""Tests of erp-api-reader.py against a local stand-in for the API, never the real one.

    uv run python -m unittest scripts/test_erp_api_reader.py
"""

import importlib.util
import json
import os
import stat
import sys
import tempfile
import threading
import time
import unittest
from datetime import date
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

SCRIPT = Path(__file__).with_name("erp-api-reader.py")
spec = importlib.util.spec_from_file_location("erp_api_reader", SCRIPT)
tool = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = tool
spec.loader.exec_module(tool)


class StandIn:
    """Answers each request from a script of (status, body) pairs, one list per path, and records what it saw."""

    def __init__(self, script: dict):
        self.script = script
        self.seen = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def _answer(self):
                path = self.path.split("?")[0]
                length = int(self.headers.get("Content-Length", 0))
                outer.seen.append((self.command, self.path, dict(self.headers), self.rfile.read(length).decode()))
                status, body = outer.script[path].pop(0)
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(json.dumps(body).encode())

            do_GET = do_POST = _answer

            def log_message(self, *_):
                pass

        self.server = HTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_port}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self):
        self.server.shutdown()
        self.server.server_close()


class ErpApiReaderTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.token_path = Path(self.dir.name) / "nested" / "token.json"
        os.environ["ERP_CLIENT_ID"] = "the-id"
        os.environ["ERP_CLIENT_SECRET"] = "the-secret"
        self.standins = []

    def tearDown(self):
        for standin in self.standins:
            standin.close()
        self.dir.cleanup()

    def standin(self, script):
        made = StandIn(script)
        self.standins.append(made)
        return made

    def tokens(self, expires_in=3600, refresh="r1"):
        tool.save_tokens(
            {"access_token": "a1", "refresh_token": refresh, "expires_at": time.time() + expires_in}, self.token_path
        )

    def client(self, api, auth=None):
        return tool.Client(api.url, self.token_path, (auth.url + "/token") if auth else "unused", sleep=lambda _: None)

    def test_the_token_file_is_private(self):
        self.tokens()
        self.assertEqual(stat.S_IMODE(self.token_path.stat().st_mode), 0o600)

    def test_a_valid_token_is_used_without_asking_for_another(self):
        self.tokens()
        self.assertEqual(tool.fresh_access_token(self.token_path, "http://unused.invalid"), "a1")

    def test_an_expired_token_is_renewed_with_basic_credentials_and_kept(self):
        self.tokens(expires_in=0)
        auth = self.standin({"/token": [(200, {"access_token": "a2", "expires_in": 60})]})
        token = tool.fresh_access_token(self.token_path, auth.url + "/token")
        self.assertEqual(token, "a2")
        _, _, headers, body = auth.seen[0]
        self.assertTrue(headers["Authorization"].startswith("Basic "))
        self.assertIn("grant_type=refresh_token", body)
        self.assertIn("refresh_token=r1", body)
        kept = tool.load_tokens(self.token_path)
        self.assertEqual((kept["access_token"], kept["refresh_token"]), ("a2", "r1"))

    def test_the_secret_never_reaches_the_error_of_a_refused_token_request(self):
        self.tokens(expires_in=0)
        auth = self.standin({"/token": [(400, {"error": "invalid_grant"})]})
        with self.assertRaises(tool.ErpApiError) as raised:
            tool.fresh_access_token(self.token_path, auth.url + "/token")
        self.assertIn("invalid_grant", str(raised.exception))
        self.assertNotIn("the-secret", str(raised.exception))

    def test_pages_are_read_until_a_short_one_whether_a_list_or_an_object(self):
        self.tokens()
        api = self.standin(
            {"/v1/pessoa": [(200, [{"id": 1}, {"id": 2}]), (200, [{"id": 3}])],
             "/v1/x": [(200, {"itens": [{"id": 1}, {"id": 2}]}), (200, {"itens": []})]}
        )
        client = self.client(api)
        self.assertEqual(client.pages("/v1/pessoa", page_size=2), [{"id": 1}, {"id": 2}, {"id": 3}])
        self.assertEqual(client.pages("/v1/x", page_size=2), [{"id": 1}, {"id": 2}])
        self.assertIn("pagina=2&tamanho_pagina=2", api.seen[1][1])

    def test_a_401_renews_the_token_once_and_tries_again(self):
        self.tokens()
        auth = self.standin({"/token": [(200, {"access_token": "a2", "expires_in": 60})]})
        api = self.standin({"/v1/pessoa": [(401, {}), (200, [])]})
        self.client(api, auth).pages("/v1/pessoa")
        self.assertEqual(api.seen[0][2]["Authorization"], "Bearer a1")
        self.assertEqual(api.seen[1][2]["Authorization"], "Bearer a2")

    def test_a_429_and_a_5xx_are_waited_out_and_a_4xx_stops(self):
        self.tokens()
        api = self.standin({"/v1/pessoa": [(429, {}), (503, {}), (200, [{"id": 1}])], "/v1/bad": [(404, {"message": "no"})]})
        client = self.client(api)
        self.assertEqual(client.pages("/v1/pessoa"), [{"id": 1}])
        with self.assertRaises(tool.ErpApiError):
            client.pages("/v1/bad")

    def test_the_code_is_taken_from_the_address_the_browser_landed_on_or_given_bare(self):
        landed = "https://example.org/?code=abc123&state=S"
        self.assertEqual(tool.code_from_callback(landed, "S"), "abc123")
        self.assertEqual(tool.code_from_callback(" abc123 ", "S"), "abc123")
        with self.assertRaises(tool.ErpApiError):
            tool.code_from_callback(landed, "OTHER")
        with self.assertRaises(tool.ErpApiError):
            tool.code_from_callback("https://example.org/?code=", "S")

    def test_the_registered_address_may_sit_behind_a_hash_route(self):
        url = "https://login.example.org/#/oauth/authorize?response_type=code&client_id=i&redirect_uri=https://example.org&state=ESTADO"
        parameters = tool.authorization_parameters(url)
        self.assertEqual((parameters["redirect_uri"], parameters["state"]), ("https://example.org", "ESTADO"))

    def test_an_env_file_fills_only_what_the_environment_lacks(self):
        path = Path(self.dir.name) / "env"
        path.write_text("# c\nERP_CLIENT_ID=from-file\nERP_X=\"quoted\"\n")
        os.environ.pop("ERP_X", None)
        tool.load_env_file(path)
        self.assertEqual(os.environ["ERP_CLIENT_ID"], "the-id")
        self.assertEqual(os.environ["ERP_X"], "quoted")

    def test_a_dropped_connection_is_tried_again(self):
        self.tokens()
        api = self.standin({"/v1/pessoas": [(200, [{"id": 1}])]})
        client = self.client(api)
        real = tool.urllib.request.urlopen
        calls = []

        def flaky(*args, **kwargs):
            calls.append(1)
            if len(calls) == 1:
                raise ConnectionResetError("reset")
            return real(*args, **kwargs)

        tool.urllib.request.urlopen = flaky
        try:
            self.assertEqual(client.pages("/v1/pessoas"), [{"id": 1}])
        finally:
            tool.urllib.request.urlopen = real

    def test_windows_cover_the_span_once_without_overlap(self):
        spans = list(tool.windows(date(2024, 1, 1), date(2024, 12, 31), days=90))
        self.assertEqual(spans[0][0], date(2024, 1, 1))
        self.assertEqual(spans[-1][1], date(2024, 12, 31))
        for (_, last), (first, _) in zip(spans, spans[1:]):
            self.assertEqual((first - last).days, 1)
        self.assertEqual(sum((last - first).days + 1 for first, last in spans), 366)

    def test_fetch_writes_one_file_per_register(self):
        self.tokens()
        listing = "/v1/financeiro/eventos-financeiros/contas-a-pagar/buscar"
        receivable = "/v1/financeiro/eventos-financeiros/contas-a-receber/buscar"
        empty = lambda: [(200, {"itens": []})]
        api = self.standin(
            {
                "/v1/categorias": [(200, {"itens": [{"id": "c"}]}), (200, {"itens": []})],
                "/v1/centro-de-custo": [(200, {"items": [{"id": "cc"}]})],
                "/v1/financeiro/transferencias": empty(),
                "/v1/financeiro/eventos-financeiros/saldo-inicial": empty(),
                "/v1/financeiro/eventos-financeiros/parcelas/p": [(200, {"id": "p", "baixas": []})],
                "/v1/conta-financeira": empty(),
                "/v1/pessoas": [(200, {"items": [{"id": "x"}]})] + [(200, {"items": []})] * 8,
                "/v1/produtos": empty() * 2,
                receivable: empty(),
                listing: [(200, {"itens": [{"id": "p"}]})],
            }
        )
        out = Path(self.dir.name) / "out"
        counts = tool.fetch(self.client(api), date(2024, 1, 1), date(2024, 1, 31), out)
        self.assertEqual(counts["categories-income"], 1)
        self.assertEqual(counts["payables"], 1)
        self.assertEqual(counts["cost-centers"], 1)
        self.assertEqual(counts["people"], 1)
        self.assertEqual(json.loads((out / "installments" / "p.json").read_text())["id"], "p")
        self.assertEqual(json.loads((out / "payables.json").read_text()), [{"id": "p"}])
        self.assertEqual(len(list(out.glob("*.json"))), 10)


if __name__ == "__main__":
    unittest.main()
