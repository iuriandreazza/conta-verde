#!/usr/bin/env python3
"""Turns what `erp-api-reader.py fetch` wrote into what conta-verde takes (ADR 0080).

    uv run scripts/erp-to-conta-verde.py plan --data-dir out/erp --out-dir out/plan

`plan` reads the JSON files and writes, without touching any server:
  - `registers.json`: the categories (with the groups conta-verde needs on a root), cost centers and bank accounts to
    create, the transfers and the opening balances;
  - `parties.csv` (ADR 0025) and `entries.csv` (ADR 0064), the files the importers take;
  - `report.txt`: everything that did not carry over as it was, so a person decides before anything is loaded.

Money is handled as whole cents, never as a float. Nothing is sent anywhere.
"""

import argparse
import csv
import http.cookiejar
import json
import os
import re
import sys
import urllib.error
import urllib.parse
import urllib.request
from collections import Counter, defaultdict
from decimal import ROUND_HALF_UP, Decimal
from pathlib import Path

# What the old system calls a group of the income statement, and the two groups conta-verde keeps on a root
# category (ADR 0012): (DRE group, cash flow group). A category the old system leaves out of its statement has no
# entry here and goes outside the result.
DRE_GROUPS = {
    "RECEITA_VENDA_PRODUTOS_SERVICOS": ("gross_revenue", "operating"),
    "RECEITA_FRETES_ENTREGAS": ("gross_revenue", "operating"),
    "RECEITAS_RENDIMENTOS_FINANCEIROS": ("financial_result", "operating"),
    "OUTRAS_RECEITAS_NAO_OPERACIONAIS": ("financial_result", "operating"),
    "IMPOSTOS_SOBRE_VENDAS": ("deductions", "operating"),
    "DESCONTOS_INCONDICIONAIS": ("deductions", "operating"),
    "CUSTO_SERVICOS_PRESTADOS": ("costs", "operating"),
    "COMISSOES_SOBRE_VENDAS": ("operating_expenses", "operating"),
    "DESPESAS_COMERCIAIS": ("operating_expenses", "operating"),
    "DESPESAS_ADMINISTRATIVAS": ("operating_expenses", "operating"),
    "DESPESAS_OPERACIONAIS_NIVEL_2": ("operating_expenses", "operating"),
    "DESPESSAS_FINANCEIRAS": ("financial_result", "operating"),
    "OUTRAS_DESPESAS_NAO_OPERACIONAIS": ("financial_result", "operating"),
    "INVESTIMENTOS_IMOBILIZADO": ("investments", "investing"),
    "EMPRESTIMOS_DIVIDAS": ("outside_result", "financing"),
}
OUTSIDE_THE_STATEMENT = ("outside_result", "operating")
ACCOUNT_KINDS = {
    "CONTA_CORRENTE": "checking",
    "CARTAO_CREDITO": "card",
    "APLICACAO": "savings",
    "INVESTIMENTO": "savings",
    "POUPANCA": "savings",
}
ENTRY_COLUMNS = [
    "reference", "kind", "description", "party_document", "party_name", "category", "cost_center",
    "competence_date", "due_date", "amount", "settled_on", "applied", "interest", "fine", "discount", "bank_account",
]
PARTY_COLUMNS = [
    "name", "document", "email", "customer", "supplier", "carrier", "seller", "postal_code", "street", "number",
    "complement", "district", "city", "state", "tags",
]
STATES = set("AC AL AM AP BA CE DF ES GO MA MG MS MT PA PB PE PI PR RJ RN RO RR RS SC SE SP TO".split())
PROFILES = {"Cliente": "customer", "Fornecedor": "supplier", "Transportadora": "carrier"}
ROLE_OF_KIND = {"RECEITA": "receivable", "DESPESA": "payable"}
SEPARATOR = " > "
EMAIL = re.compile(r"^[^@\s]+@[^@\s]+\.[^@\s]+$")


def to_cents(value) -> int:
    return int((Decimal(str(value or 0)) * 100).quantize(Decimal(1), rounding=ROUND_HALF_UP))


def money(cents: int) -> str:
    sign = "-" if cents < 0 else ""
    return f"{sign}{abs(cents) // 100}.{abs(cents) % 100:02d}"


def split_cents(total: int, weights: list[int]) -> list[int]:
    """`total` shared in proportion to `weights`, each share cut to the cent and the last taking the rest."""
    weight_sum = sum(weights)
    shares, given = [], 0
    for weight in weights[:-1]:
        share = total * weight // weight_sum
        shares.append(share)
        given += share
    shares.append(total - given)
    return shares


def check_digit(digits: list[int], weights: list[int]) -> int:
    remainder = sum(d * w for d, w in zip(digits, weights)) % 11
    return 0 if remainder < 2 else 11 - remainder


def valid_cpf(text: str) -> bool:
    if len(text) != 11 or not text.isdigit() or len(set(text)) == 1:
        return False
    digits = [int(c) for c in text]
    first = check_digit(digits[:9], list(range(10, 1, -1)))
    return first == digits[9] and check_digit(digits[:10], list(range(11, 1, -1))) == digits[10]


def valid_cnpj(text: str) -> bool:
    """14 characters, the first 12 digits or upper case letters (value is the code point less 48)."""
    if len(text) != 14 or not re.fullmatch(r"[0-9A-Z]{12}[0-9]{2}", text) or len(set(text)) == 1:
        return False
    digits = [ord(c) - 48 for c in text]
    first = check_digit(digits[:12], [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2])
    return first == digits[12] and check_digit(digits[:13], [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]) == digits[13]


def clean_document(raw: str | None) -> str:
    return re.sub(r"[^0-9A-Za-z]", "", raw or "").upper()


class Report:
    def __init__(self):
        self.lines: list[str] = []
        self.counts: Counter = Counter()

    def note(self, kind: str, detail: str = ""):
        self.counts[kind] += 1
        if detail:
            self.lines.append(f"{kind}: {detail}")

    def text(self) -> str:
        summary = [f"{count:6d}  {kind}" for kind, count in sorted(self.counts.items())]
        return "\n".join(summary + [""] + self.lines) + "\n"


def unique_names(items: list[dict], report: Report, what: str) -> dict[str, str]:
    """id -> name, with a counter added to a name already taken (conta-verde keeps names unique, ignoring case)."""
    taken: dict[str, int] = {}
    names = {}
    for item in sorted(items, key=lambda i: (not i.get("ativo", True), i["nome"].casefold(), i["id"])):
        base = " ".join(item["nome"].split())
        key = base.casefold()
        taken[key] = taken.get(key, 0) + 1
        name = base if taken[key] == 1 else f"{base} ({taken[key]})"
        if taken[key] > 1:
            report.note(f"{what} with a repeated name, renamed", f"{base!r} -> {name!r}")
        names[item["id"]] = name
    return names


def plan_categories(income: list[dict], expense: list[dict], report: Report, chosen_groups: dict | None = None):
    """The tree conta-verde needs: a root chooses the groups, so a root takes the group most of its descendants have."""
    registers, paths = [], {}
    # One set for both kinds: conta-verde has a single tree of roots, so an income root and an expense root cannot share a name.
    taken_roots: set[str] = set()
    matched_groups: set[str] = set()
    for kind, items in (("RECEITA", income), ("DESPESA", expense)):
        by_id = {item["id"]: item for item in items}
        children = defaultdict(list)
        for item in items:
            if item.get("categoria_pai") in by_id:
                children[item["categoria_pai"]].append(item)

        def descendants(item):
            for kid in children.get(item["id"], []):
                yield kid
                yield from descendants(kid)

        def add(item, parent_path, taken):
            name = " ".join(item["nome"].split())
            if name.casefold() in taken:
                report.note("category with a repeated name among its siblings, renamed", f"{parent_path or ''}{name!r}")
                name = f"{name} ({len(taken)})"
            taken.add(name.casefold())
            path = f"{parent_path}{SEPARATOR}{name}" if parent_path else name
            paths[item["id"]] = path
            return name, path

        for root in sorted((i for i in items if i.get("categoria_pai") not in by_id), key=lambda r: r["nome"].casefold()):
            name, path = add(root, None, taken_roots)
            votes = Counter(d.get("entrada_dre") for d in descendants(root) if d.get("entrada_dre"))
            chosen = root.get("entrada_dre") or (votes.most_common(1)[0][0] if votes else None)
            groups = DRE_GROUPS.get(chosen, OUTSIDE_THE_STATEMENT)
            if name in (chosen_groups or {}):
                groups = tuple(chosen_groups[name])
                matched_groups.add(name)
            elif chosen is None:
                report.note("root category without a group, put outside the result", repr(name))
            for key in votes:
                if DRE_GROUPS.get(key, OUTSIDE_THE_STATEMENT) != groups:
                    report.note("descendant group differs from its root's", f"{name!r}: {key} (root takes {groups[0]})")
            registers.append({"name": name, "kind": kind, "parent": None, "dre_group": groups[0], "cash_flow_group": groups[1]})

            def walk(item, parent_path):
                taken: set[str] = set()
                for kid in sorted(children.get(item["id"], []), key=lambda k: k["nome"].casefold()):
                    kid_name, kid_path = add(kid, parent_path, taken)
                    registers.append({"name": kid_name, "kind": kind, "parent": parent_path})
                    walk(kid, kid_path)

            walk(root, path)
    for unknown in sorted(set(chosen_groups or {}) - matched_groups):
        # A typo here would leave a root outside the result, and groups cannot change once the category exists.
        report.note("--groups name that is no root category", repr(unknown))
    return registers, paths


def plan_accounts(accounts: list[dict], report: Report):
    names = unique_names(accounts, report, "bank account")
    registers = []
    for account in accounts:
        code = account.get("codigo_banco")
        bank_code = f"{code:03d}" if isinstance(code, int) and 0 < code < 1000 else None
        kind = ACCOUNT_KINDS.get(account["tipo"], "cash")
        if kind == "cash" and account["tipo"] not in ("CAIXINHA", "OUTROS", "MEIOS_RECEBIMENTO"):
            report.note("account kind treated as cash", f"{names[account['id']]!r} is {account['tipo']}")
        registers.append({
            "id": account["id"], "name": names[account["id"]], "kind": kind, "active": bool(account["ativo"]),
            "bank_code": bank_code if kind != "cash" else None,
            "branch": re.sub(r"[^0-9A-Za-z]", "", account.get("agencia") or "").upper() or None,
            "number": re.sub(r"[^0-9A-Za-z]", "", account.get("numero") or "").upper() or None,
        })
    return registers, names


def plan_parties(people: list[dict], report: Report):
    """Rows for the parties import, and id -> (document, name) so an entry can name its party."""
    rows, by_id, documents = [], {}, set()
    for person in people:
        document = clean_document(person.get("documento"))
        if document and not (valid_cpf(document) or valid_cnpj(document)):
            report.note("party document refused by the check digits, left blank", f"{person['nome']!r}: {document}")
            document = ""
        if document and document in documents:
            report.note("party document already on another party, left blank", f"{person['nome']!r}: {document}")
            document = ""
        documents.add(document)
        email = (person.get("email") or "").strip()
        if email and not EMAIL.match(email):
            report.note("party e-mail dropped (not an address)", repr(person["nome"]))
            email = ""
        if not person.get("ativo", True):
            report.note("party inactive in the old system, imported active", repr(person["nome"]))
        profiles = {PROFILES[p] for p in person.get("perfis", []) if p in PROFILES}
        row = {column: "" for column in PARTY_COLUMNS}
        row.update({"name": " ".join(person["nome"].split()), "document": document, "email": email})
        for role in ("customer", "supplier", "carrier", "seller"):
            row[role] = "sim" if role in profiles else "não"
        address = person.get("endereco") if isinstance(person.get("endereco"), dict) else {}
        cep = re.sub(r"\D", "", address.get("cep") or "")
        state = (address.get("estado") or "").strip().upper()
        if address and len(cep) == 8 and address.get("logradouro") and address.get("cidade") and state in STATES:
            row.update({"postal_code": cep, "state": state})
            for column, key, limit in (("street", "logradouro", 150), ("number", "numero", 20), ("complement", "complemento", 80),
                                       ("district", "bairro", 80), ("city", "cidade", 80)):
                text = " ".join((address.get(key) or "").split())
                if len(text) > limit:
                    report.note(f"party address {column} cut to {limit} characters", repr(person["nome"]))
                row[column] = text[:limit]
        elif any(address.get(k) for k in ("cep", "logradouro", "cidade")):
            report.note("party address incomplete, left out", repr(person["nome"]))
        rows.append(row)
        by_id[person["id"]] = (document, row["name"])
    return rows, by_id


def pieces_of(detail: dict, total: int, categories: dict, centers: dict, report: Report):
    """(category path, cost center name or None, cents) pieces that add up to the installment's `total`.

    The split lives on the event, which may have several installments, so it is applied in proportion."""
    weighted = []
    for share in detail["evento"].get("rateio") or []:
        path = categories.get(share["id_categoria"])
        if path is None:
            report.note("category not in the register", f"{share.get('nome_categoria')!r}")
            continue
        whole = to_cents(share["valor"])
        per_center = [(centers.get(c["id_centro_custo"]), to_cents(c["valor"])) for c in share.get("rateio_centro_custo") or []]
        if per_center and sum(v for _, v in per_center) == whole:
            weighted.extend((path, name, value) for name, value in per_center if value > 0)
        else:
            if per_center:
                report.note("cost centers do not add up to their category, left out", f"installment {detail['id']}")
            weighted.append((path, None, whole))
    weighted = [piece for piece in weighted if piece[2] > 0]
    if not weighted:
        return []
    shares = split_cents(total, [value for _, _, value in weighted])
    return [(path, name, share) for (path, name, _), share in zip(weighted, shares) if share > 0]


def plan_entries(listings: list[dict], details: dict, party_of: dict, categories: dict, centers: dict, accounts: dict, report: Report):
    rows = []
    for item in listings:
        detail = details.get(item["id"])
        if detail is None:
            report.note("installment without its detail, left out", item["id"])
            continue
        if detail["status"] == "PERDIDO" and detail.get("baixas"):
            report.note("lost installment that was paid in part, imported as paid", f"{item['id']} {detail['descricao']!r}")
        elif detail["status"] not in ("QUITADO", "PENDENTE"):
            report.note(f"installment {detail['status']}, left out", f"{item['id']} {detail['descricao']!r}")
            continue
        kind = ROLE_OF_KIND[detail["evento"]["tipo"]]
        party = item.get("cliente") or item.get("fornecedor") or {}
        document, party_name = party_of.get(party.get("id"), ("", " ".join((party.get("nome") or "").split())))
        paid = to_cents(detail["valor_pago"])
        total = paid + to_cents(detail["nao_pago"])
        settlement = {"applied": 0, "interest": 0, "fine": 0, "discount": 0}
        settled_on = account = ""
        if detail["status"] == "QUITADO" or detail.get("baixas"):
            payments = detail.get("baixas") or []
            if not payments:
                report.note("paid installment without a payment, left out", item["id"])
                continue
            if len(payments) > 1:
                report.note("installment paid more than once, joined into one settlement", item["id"])
            parts = [p["valor_composicao"] for p in payments]
            settlement = {
                "applied": sum(to_cents(p["valor_bruto"]) for p in parts), "interest": sum(to_cents(p["juros"]) for p in parts),
                "fine": sum(to_cents(p["multa"]) for p in parts), "discount": sum(to_cents(p["desconto"]) for p in parts),
            }
            fee = sum(to_cents(p.get("taxa")) for p in parts)
            if fee:
                # The old system takes the fee out of the cash; a discount on the settlement does the same, so the
                # account's balance agrees while the installment is still settled in full.
                settlement["discount"] += fee
                report.note("payment fee carried as a discount on the settlement", item["id"])
            last = max(payments, key=lambda p: p["data_pagamento"])
            settled_on = last["data_pagamento"]
            account = accounts.get(last["conta_financeira"]["id"], "")
            if not account:
                report.note("payment account not in the register, left out", item["id"])
                continue
            if settlement["applied"] != total:
                report.note("applied differs from the installment amount", item["id"])
        if total == 0:
            report.note("installment worth zero, left out", f"{item['id']} {detail['descricao']!r}")
            continue
        pieces = pieces_of(detail, total, categories, centers, report)
        if not pieces:
            report.note("installment without a usable category split, left out", item["id"])
            continue
        applied_shares = split_cents(settlement["applied"], [c for _, _, c in pieces])
        extras = {key: split_cents(settlement[key], [c for _, _, c in pieces]) for key in ("interest", "fine", "discount")}
        for number, (path, center, cents) in enumerate(pieces):
            row = dict.fromkeys(ENTRY_COLUMNS, "")
            row.update({
                "reference": item["id"] if len(pieces) == 1 else f"{item['id']}:{number + 1}", "kind": kind,
                "description": " ".join(detail["descricao"].split()), "party_document": document, "party_name": party_name,
                "category": path, "cost_center": center or "", "competence_date": detail["evento"]["data_competencia"],
                "due_date": detail["data_vencimento"], "amount": money(cents),
            })
            if settled_on:
                row.update({
                    "settled_on": settled_on, "applied": money(applied_shares[number]), "interest": money(extras["interest"][number]),
                    "fine": money(extras["fine"][number]), "discount": money(extras["discount"][number]), "bank_account": account,
                })
            rows.append(row)
    return rows


def plan_transfers(transfers: list[dict], accounts: dict, report: Report):
    out = []
    for transfer in transfers:
        source, target = transfer["origem"]["conta_financeira"]["id"], transfer["destino"]["conta_financeira"]["id"]
        if source not in accounts or target not in accounts:
            report.note("transfer with an account not in the register, left out", transfer["id"])
            continue
        if to_cents(transfer["origem"].get("composicao_valor", {}).get("valor_bruto") or transfer["valor"]) != to_cents(transfer["valor"]):
            report.note("transfer whose two sides differ in value, taken at its value", transfer["id"])
        out.append({
            "from": accounts[source], "to": accounts[target], "amount": money(to_cents(transfer["valor"])),
            "occurred_on": transfer["data"], "note": (transfer.get("descricao") or "")[:200] or None,
        })
    return out


def first_movement_days(entries: list[dict], transfers: list[dict]) -> dict[str, str]:
    """The earliest day each account (by name) has a payment or a transfer."""
    days: dict[str, str] = {}
    moves = [(row["bank_account"], row["settled_on"]) for row in entries if row["settled_on"]]
    moves += [(name, transfer["occurred_on"]) for transfer in transfers for name in (transfer["from"], transfer["to"])]
    for name, day in moves:
        days[name] = min(day, days.get(name, day))
    return days


def plan_openings(openings: list[dict], accounts: dict, first_moves: dict[str, str], report: Report):
    """conta-verde counts an account's movements only from its opening day, while the old system counts them all, so an
    opening that came after the first movement is brought back to it (its amount is what the account held before)."""
    out = []
    for opening in openings:
        name = accounts.get(opening["id_conta_financeira"])
        if name is None:
            report.note("opening balance of an account not in the register, left out", opening["id_conta_financeira"])
            continue
        cents = to_cents(opening["saldo_inicial"]) * (-1 if opening["tipo"] == "DESPESA" else 1)
        day = min(opening["data_competencia"], first_moves.get(name, opening["data_competencia"]))
        if day != opening["data_competencia"]:
            report.note("opening balance moved back to the account's first movement", f"{name!r}: {opening['data_competencia']} -> {day}")
        out.append({"account": name, "balance": money(cents), "on": day})
    return out


def write_csv(path: Path, columns: list[str], rows: list[dict]):
    with path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns, delimiter=";", lineterminator="\r\n")
        writer.writeheader()
        writer.writerows(rows)


def plan(data_dir: Path, out_dir: Path, groups_file: Path | None = None) -> Report:
    read = lambda name: json.loads((data_dir / f"{name}.json").read_text())
    report = Report()
    out_dir.mkdir(parents=True, exist_ok=True)
    chosen_groups = json.loads(groups_file.read_text()) if groups_file else None
    categories, paths = plan_categories(read("categories-income"), read("categories-expense"), report, chosen_groups)
    cost_centers = read("cost-centers")
    center_names = unique_names(cost_centers, report, "cost center")
    accounts, account_names = plan_accounts(read("financial-accounts"), report)
    parties, party_of = plan_parties(read("people"), report)
    details = {p.stem: json.loads(p.read_text()) for p in (data_dir / "installments").glob("*.json")}
    entries = plan_entries(
        read("receivables") + read("payables"), details, party_of, paths, center_names, account_names, report
    )
    transfers = plan_transfers(read("transfers"), account_names, report)
    registers = {
        "categories": categories,
        "cost_centers": [{"name": center_names[c["id"]], "active": bool(c["ativo"])} for c in cost_centers],
        "bank_accounts": accounts,
        "transfers": transfers,
        "opening_balances": plan_openings(
            read("opening-balances"), account_names, first_movement_days(entries, transfers), report
        ),
    }
    (out_dir / "registers.json").write_text(json.dumps(registers, ensure_ascii=False, indent=1))
    write_csv(out_dir / "parties.csv", PARTY_COLUMNS, parties)
    write_csv(out_dir / "entries.csv", ENTRY_COLUMNS, entries)
    report.counts["== parties"] = len(parties)
    report.counts["== entry rows"] = len(entries)
    for name, rows in registers.items():
        report.counts[f"== {name}"] = len(rows)
    (out_dir / "report.txt").write_text(report.text())
    return report


class LoadError(Exception):
    pass


class Api:
    """One person's session on a running conta-verde API: a cookie jar and the JSON it speaks."""

    def __init__(self, base: str):
        self.base = base.rstrip("/")
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def call(self, method: str, path: str, body=None, raw: bytes | None = None, content_type: str = "application/json"):
        data = raw if raw is not None else (None if body is None else json.dumps(body).encode())
        request = urllib.request.Request(self.base + path, data=data, method=method)
        if data is not None:
            request.add_header("Content-Type", content_type)
        try:
            with self.opener.open(request, timeout=300) as response:
                payload = response.read()
                return response.status, json.loads(payload) if payload else None
        except urllib.error.HTTPError as error:
            payload = error.read()
            return error.code, json.loads(payload) if payload.startswith(b"{") else payload.decode(errors="replace")

    def ok(self, method: str, path: str, body=None, raw: bytes | None = None, content_type: str = "application/json"):
        status, answer = self.call(method, path, body, raw, content_type)
        if status >= 300:
            raise LoadError(f"{method} {path} answered {status}: {str(answer)[:400]}")
        return answer

    def listing(self, path: str) -> list:
        items, after = [], None
        while True:
            query = "limit=100" + (f"&after={urllib.parse.quote(after)}" if after else "")
            answer = self.ok("GET", f"{path}?{query}")
            items.extend(answer["items"])
            after = answer.get("next_cursor")
            if not after:
                return items


MAX_IMPORT_BYTES = 900_000
MAX_IMPORT_ROWS = 2000


def chunks_of(csv_text: str):
    """The file cut into pieces the importers accept (1 MiB, 2000 rows), each with the header again."""
    header, *lines = csv_text.splitlines(keepends=True)
    piece, size = [], len(header)
    for line in lines:
        if piece and (size + len(line) > MAX_IMPORT_BYTES or len(piece) >= MAX_IMPORT_ROWS):
            yield header + "".join(piece)
            piece, size = [], len(header)
        piece.append(line)
        size += len(line)
    if piece:
        yield header + "".join(piece)


def import_file(api: Api, base: str, path: Path, label: str):
    """Every piece is checked first, so a refused row stops the load before the first write."""
    pieces = [chunk.encode("utf-8") for chunk in chunks_of(path.read_text(encoding="utf-8"))]
    for dry in (True, False):
        created = 0
        for number, piece in enumerate(pieces, 1):
            answer = api.ok("POST", f"{base}?dry_run={'true' if dry else 'false'}", raw=piece, content_type="text/csv")
            if answer["refused"]:
                shown = "; ".join(f"row {n['row']}: {n['message']}" for n in answer["refused"][:10])
                raise LoadError(f"{label}, piece {number}: {len(answer['refused'])} rows refused: {shown}")
            created += answer["created"]
        print(f"{label}: {'checked' if dry else f'created {created}'} ({len(pieces)} pieces)", file=sys.stderr)


def category_ids_by_path(categories: list[dict]) -> dict[str, str]:
    by_id = {category["id"]: category for category in categories}
    paths = {}
    for category in categories:
        chain, node = [category["name"]], category
        while node["parent_id"]:
            node = by_id[node["parent_id"]]
            chain.append(node["name"])
        paths[SEPARATOR.join(reversed(chain))] = category["id"]
    return paths


def load_categories(api: Api, base: str, registers: list[dict]):
    categories = api.ok("GET", f"{base}/categories")["items"]
    paths = category_ids_by_path(categories)
    if not any(c["dre_group"] == "transfers" for c in categories):
        # Moves between the company's own accounts need the one category of the transfers group.
        made = api.ok("POST", f"{base}/categories", {"name": "Transferências", "dre_group": "transfers", "cash_flow_group": "transfer"})
        paths[made["name"]] = made["id"]
    for category in registers:
        full = f"{category['parent']}{SEPARATOR}{category['name']}" if category["parent"] else category["name"]
        if full in paths:
            continue
        body = {"name": category["name"]}
        if category["parent"]:
            body["parent_id"] = paths[category["parent"]]
        else:
            body.update({"dre_group": category["dre_group"], "cash_flow_group": category["cash_flow_group"]})
        paths[full] = api.ok("POST", f"{base}/categories", body)["id"]


def load(api: Api, company: str, plan_dir: Path) -> None:
    registers = json.loads((plan_dir / "registers.json").read_text())
    base = f"/companies/{company}"
    # Everything is made active, because the importers and transfers only reach active records; what was
    # inactive in the old system is switched off at the end.
    load_categories(api, base, registers["categories"])
    print(f"categories: {len(registers['categories'])} in place", file=sys.stderr)

    centers = {c["name"].casefold(): c for c in api.listing(f"{base}/cost-centers")}
    for center in registers["cost_centers"]:
        if center["name"].casefold() not in centers:
            centers[center["name"].casefold()] = api.ok("POST", f"{base}/cost-centers", {"name": center["name"]})
    print(f"cost centers: {len(registers['cost_centers'])} in place", file=sys.stderr)

    account_fields = ("kind", "name", "bank_code", "branch", "number")
    accounts = {a["name"].casefold(): a for a in api.listing(f"{base}/bank-accounts")}
    for account in registers["bank_accounts"]:
        if account["name"].casefold() not in accounts:
            accounts[account["name"].casefold()] = api.ok("POST", f"{base}/bank-accounts", {k: account[k] for k in account_fields})
    print(f"bank accounts: {len(registers['bank_accounts'])} in place", file=sys.stderr)

    # A party without a document has nothing to tell it from itself, so sending the file again would register it
    # twice: the file goes once, into a company that has no party yet.
    if api.ok("GET", f"{base}/parties?limit=1")["items"]:
        print("parties: the company already has some, not imported again", file=sys.stderr)
    else:
        import_file(api, f"{base}/parties/import", plan_dir / "parties.csv", "parties")
    import_file(api, f"{base}/entries/import", plan_dir / "entries.csv", "entries")

    # The API writes an amount without trailing zeros, so amounts are compared as numbers.
    known = {(t["from_account_id"], t["to_account_id"], Decimal(t["amount"]), t["occurred_on"]) for t in api.listing(f"{base}/transfers")}
    made = 0
    for transfer in registers["transfers"]:
        body = {
            "from_account_id": accounts[transfer["from"].casefold()]["id"], "to_account_id": accounts[transfer["to"].casefold()]["id"],
            "amount": transfer["amount"], "occurred_on": transfer["occurred_on"], "note": transfer["note"],
        }
        if (body["from_account_id"], body["to_account_id"], Decimal(body["amount"]), body["occurred_on"]) not in known:
            api.ok("POST", f"{base}/transfers", body)
            made += 1
    print(f"transfers: created {made} of {len(registers['transfers'])}", file=sys.stderr)

    for opening in registers["opening_balances"]:
        account = accounts[opening["account"].casefold()]
        api.ok("PUT", f"{base}/bank-accounts/{account['id']}/opening-balance", {"balance": opening["balance"], "on": opening["on"]})
    print(f"opening balances: {len(registers['opening_balances'])} set", file=sys.stderr)

    for center in registers["cost_centers"]:
        if not center["active"]:
            record = centers[center["name"].casefold()]
            api.ok("PUT", f"{base}/cost-centers/{record['id']}", {"name": record["name"], "active": False})
    for account in registers["bank_accounts"]:
        if not account["active"]:
            record = accounts[account["name"].casefold()]
            api.ok("PUT", f"{base}/bank-accounts/{record['id']}", {**{k: record.get(k) for k in account_fields}, "active": False})
    print("inactive cost centers and accounts switched off", file=sys.stderr)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    plan_parser = commands.add_parser("plan", help="write the files conta-verde takes and a report")
    plan_parser.add_argument("--data-dir", type=Path, required=True)
    plan_parser.add_argument("--out-dir", type=Path, required=True)
    plan_parser.add_argument(
        "--groups", type=Path, help='JSON {"root category": ["dre_group", "cash_flow_group"]} for roots the old system leaves ungrouped'
    )
    load_parser = commands.add_parser("load", help="create the registers and import the files in a running conta-verde")
    load_parser.add_argument("--plan-dir", type=Path, required=True)
    load_parser.add_argument(
        "--credentials-file", type=Path, required=True,
        help='JSON {"api": "http://127.0.0.1:3055", "company_id": "...", "email": "...", "password": "..."}; a password never goes in an argument',
    )
    args = parser.parse_args(argv)
    if args.command == "plan":
        report = plan(args.data_dir, args.out_dir, args.groups)
        print("\n".join(f"{count:6d}  {kind}" for kind, count in sorted(report.counts.items())), file=sys.stderr)
        return 0
    credentials = json.loads(args.credentials_file.read_text())
    api = Api(credentials["api"])
    try:
        api.ok("POST", "/auth/login", {"email": credentials["email"], "password": credentials["password"]})
        load(api, credentials["company_id"], args.plan_dir)
    except LoadError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
