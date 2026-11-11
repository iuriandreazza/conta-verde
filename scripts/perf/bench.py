#!/usr/bin/env python3
"""Times the main endpoints against the seeded database of the performance baseline (ADR 0097, docs/performance/baseline.md).

    python3 scripts/perf/bench.py http://127.0.0.1:3193 [--runs 15] [--json results.json]

Signs in as the user scripts/perf/prepare.sh made, finds the large company, and for every endpoint below makes one warm-up call and
`--runs` timed ones, one after the other (this measures latency, not load). It prints a markdown table with the median, the 95th
percentile and the worst of each, against its budget, and exits 1 when something is over. The time is what the client sees, so it
includes the server's work for the policies, the row reads and the JSON; the network is the machine's own.

Budgets (ADR 0097): a keyset page of a list <= 200 ms; a report, the DRE, the cash flow, a dashboard card <= 1500 ms.
"""

import argparse
import http.cookiejar
import json
import math
import statistics
import sys
import time
import urllib.error
import urllib.request

PAGE = 200  # ms
REPORT = 1500  # ms
PASSWORD = "perf-password-1234"


class Client:
    def __init__(self, base):
        self.base = base.rstrip("/")
        policy = http.cookiejar.DefaultCookiePolicy(secure_protocols=("https", "http"))
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar(policy)))

    def get(self, path):
        start = time.perf_counter()
        try:
            with self.opener.open(self.base + path) as response:
                body = response.read()
                status = response.status
        except urllib.error.HTTPError as error:
            body, status = error.read(), error.code
        return status, body, (time.perf_counter() - start) * 1000

    def login(self, email, password):
        request = urllib.request.Request(
            self.base + "/auth/login", data=json.dumps({"email": email, "password": password}).encode(),
            method="POST", headers={"Content-Type": "application/json"},
        )
        with self.opener.open(request) as response:
            return json.loads(response.read())


def cases(company, entry_id, product_id):
    base = f"/companies/{company}"
    return [
        # (name, path, budget in ms)
        ("entries: first page", f"{base}/entries", PAGE),
        ("entries: first page, receivables", f"{base}/entries?kind=receivable", PAGE),
        ("entries: search", f"{base}/entries?q=Entry%201234", PAGE),
        ("entries: oldest first", f"{base}/entries?sort=competence_date", PAGE),
        ("entry: one, with its installments", f"{base}/entries/{entry_id}", PAGE),
        ("entry: its settlements", f"{base}/entries/{entry_id}/settlements", PAGE),
        ("entry: its history", f"{base}/entries/{entry_id}/history", PAGE),
        ("parties: first page", f"{base}/parties", PAGE),
        ("parties: search", f"{base}/parties?q=Party%200123", PAGE),
        ("products: first page", f"{base}/products", PAGE),
        ("sales: first page", f"{base}/sales", PAGE),
        ("sales: completed, newest", f"{base}/sales?status=completed", PAGE),
        ("stock: positions", f"{base}/stock/positions", PAGE),
        ("stock: position tiles", f"{base}/stock/positions/summary", REPORT),
        ("stock: movements", f"{base}/stock/movements", PAGE),
        ("stock: movements of a product", f"{base}/stock/movements?product={product_id}", PAGE),
        ("stock: alerts", f"{base}/stock/alerts", REPORT),
        ("balances (dashboard)", f"{base}/balances?on=2026-11-11", REPORT),
        ("cash flow, a year by month", f"{base}/cash-flow?from=2026-01-01&to=2026-12-31&granularity=month", REPORT),
        ("cash flow, 90 days by day", f"{base}/cash-flow?from=2026-09-01&to=2026-11-30&granularity=day", REPORT),
        ("DRE, a year by month", f"{base}/dre?from=2025-11-01&to=2026-10-31&by=month", REPORT),
        ("report: entries by category, a year", f"{base}/reports/entries?group=category&basis=competence&from=2025-11-01&to=2026-10-31", REPORT),
        ("report: entries by party, five years", f"{base}/reports/entries?group=party&basis=due&from=2021-11-01&to=2026-10-31", REPORT),
        ("report: overdue", f"{base}/reports/overdue?on=2026-11-11", REPORT),
        ("delinquency", f"{base}/delinquency?on=2026-11-11", REPORT),
        ("sales report, a month by customer", f"{base}/sales-reports?by=customer&from=2026-10-01&to=2026-10-31", REPORT),
        ("sales report, a month by product", f"{base}/sales-reports?by=product&from=2026-10-01&to=2026-10-31", REPORT),
        ("stock valuation", f"{base}/stock-reports/valuation?on=2026-11-11", REPORT),
        ("stock movement summary, a year", f"{base}/stock-reports/movements?from=2025-11-12&to=2026-11-11", REPORT),
        ("commissions, a month", f"{base}/commissions?from=2026-10-01&to=2026-10-31", REPORT),
    ]


def keyset_depth(client, company, depth=30):
    """The time of the 30th page of the entries list: a keyset page must cost the same however far along the list it is."""
    after, times = None, []
    for _ in range(depth):
        path = f"/companies/{company}/entries" + (f"?after={after}" if after else "")
        status, body, ms = client.get(path)
        assert status == 200, (status, body[:200])
        times.append(ms)
        after = json.loads(body).get("next_cursor")
        if not after:
            break
    return times


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("api")
    parser.add_argument("--runs", type=int, default=20)
    parser.add_argument("--json")
    arguments = parser.parse_args()

    client = Client(arguments.api)
    profile = client.login("perf@example.test", PASSWORD)
    company = next(c["id"] for c in profile["companies"] if c["name"] == "Large company")
    entry_id = json.loads(client.get(f"/companies/{company}/entries")[1])["items"][0]["id"]
    product_id = json.loads(client.get(f"/companies/{company}/products")[1])["items"][0]["id"]

    rows, over = [], 0
    for name, path, budget in cases(company, entry_id, product_id):
        status, _, _ = client.get(path)
        if status != 200:
            rows.append((name, None, None, None, budget, f"HTTP {status}"))
            over += 1
            continue
        times = sorted(client.get(path)[2] for _ in range(arguments.runs))
        p50 = statistics.median(times)
        p95 = times[max(0, math.ceil(len(times) * 0.95) - 1)]  # nearest rank: of 20 runs, the second slowest
        verdict = "ok" if p95 <= budget else "OVER"
        over += verdict != "ok"
        rows.append((name, p50, p95, times[-1], budget, verdict))

    pages = keyset_depth(client, company)
    first, last = statistics.median(pages[:3]), statistics.median(pages[-3:])
    print("| endpoint | median ms | p95 ms | worst ms | budget ms | |\n|---|---:|---:|---:|---:|---|")
    for name, p50, p95, worst, budget, verdict in rows:
        if p50 is None:
            print(f"| {name} | | | | {budget} | {verdict} |")
        else:
            print(f"| {name} | {p50:.1f} | {p95:.1f} | {worst:.1f} | {budget} | {verdict} |")
    print(f"\nkeyset depth: {len(pages)} pages of the entries list, the first three {first:.1f} ms (median), the last three {last:.1f} ms")
    if arguments.json:
        with open(arguments.json, "w") as handle:
            json.dump({"rows": rows, "keyset_pages_ms": pages}, handle)
    sys.exit(1 if over else 0)


if __name__ == "__main__":
    main()
