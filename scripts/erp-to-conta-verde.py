#!/usr/bin/env python3
"""Turns what `erp-api-reader.py fetch` wrote into what conta-verde takes (ADR 0077).

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
import json
import re
import sys
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

        taken_roots: set[str] = set()
        for root in sorted((i for i in items if i.get("categoria_pai") not in by_id), key=lambda r: r["nome"].casefold()):
            name, path = add(root, None, taken_roots)
            votes = Counter(d.get("entrada_dre") for d in descendants(root) if d.get("entrada_dre"))
            chosen = root.get("entrada_dre") or (votes.most_common(1)[0][0] if votes else None)
            groups = DRE_GROUPS.get(chosen, OUTSIDE_THE_STATEMENT)
            if name in (chosen_groups or {}):
                groups = tuple(chosen_groups[name])
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
    rows, by_id = [], {}
    for person in people:
        document = clean_document(person.get("documento"))
        if document and not (valid_cpf(document) or valid_cnpj(document)):
            report.note("party document refused by the check digits, left blank", f"{person['nome']!r}: {document}")
            document = ""
        email = (person.get("email") or "").strip()
        if email and not EMAIL.match(email):
            report.note("party e-mail dropped (not an address)", repr(person["nome"]))
            email = ""
        profiles = {PROFILES[p] for p in person.get("perfis", []) if p in PROFILES}
        row = {column: "" for column in PARTY_COLUMNS}
        row.update({"name": " ".join(person["nome"].split()), "document": document, "email": email})
        for role in ("customer", "supplier", "carrier", "seller"):
            row[role] = "sim" if role in profiles else "não"
        address = person.get("endereco") if isinstance(person.get("endereco"), dict) else {}
        cep = re.sub(r"\D", "", address.get("cep") or "")
        state = (address.get("estado") or "").strip().upper()
        if address and len(cep) == 8 and address.get("logradouro") and address.get("cidade") and state in STATES:
            row.update({
                "postal_code": cep, "street": address["logradouro"].strip(), "number": (address.get("numero") or "").strip(),
                "complement": (address.get("complemento") or "").strip(), "district": (address.get("bairro") or "").strip(),
                "city": address["cidade"].strip(), "state": state,
            })
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
        if detail["status"] not in ("QUITADO", "PENDENTE"):
            report.note(f"installment {detail['status']}, left out", f"{item['id']} {detail['descricao']!r}")
            continue
        kind = ROLE_OF_KIND[detail["evento"]["tipo"]]
        party = item.get("cliente") or item.get("fornecedor") or {}
        document, party_name = party_of.get(party.get("id"), ("", " ".join((party.get("nome") or "").split())))
        paid = to_cents(detail["valor_pago"])
        total = paid + to_cents(detail["nao_pago"])
        settlement = {"applied": 0, "interest": 0, "fine": 0, "discount": 0}
        settled_on = account = ""
        if detail["status"] == "QUITADO":
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
            if any(to_cents(p.get("taxa")) for p in parts):
                report.note("payment fee not carried over", item["id"])
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


def plan_openings(openings: list[dict], accounts: dict, report: Report):
    out = []
    for opening in openings:
        name = accounts.get(opening["id_conta_financeira"])
        if name is None:
            report.note("opening balance of an account not in the register, left out", opening["id_conta_financeira"])
            continue
        cents = to_cents(opening["saldo_inicial"]) * (-1 if opening["tipo"] == "DESPESA" else 1)
        out.append({"account": name, "balance": money(cents), "on": opening["data_competencia"]})
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
    registers = {
        "categories": categories,
        "cost_centers": [{"name": center_names[c["id"]], "active": bool(c["ativo"])} for c in cost_centers],
        "bank_accounts": accounts,
        "transfers": plan_transfers(read("transfers"), account_names, report),
        "opening_balances": plan_openings(read("opening-balances"), account_names, report),
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


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    plan_parser = commands.add_parser("plan", help="write the files conta-verde takes and a report")
    plan_parser.add_argument("--data-dir", type=Path, required=True)
    plan_parser.add_argument("--out-dir", type=Path, required=True)
    plan_parser.add_argument(
        "--groups", type=Path, help='JSON {"root category": ["dre_group", "cash_flow_group"]} for roots the old system leaves ungrouped'
    )
    args = parser.parse_args(argv)
    report = plan(args.data_dir, args.out_dir, args.groups)
    print("\n".join(f"{count:6d}  {kind}" for kind, count in sorted(report.counts.items())), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
