#!/usr/bin/env python3
"""Turns the exports of a cloud ERP into the files conta-verde imports (ADR 0063).

    uv run --with openpyxl scripts/conta-azul-to-entries-csv.py entries --kind payable contas-a-pagar.xls -o entries.csv
    uv run --with openpyxl scripts/conta-azul-to-entries-csv.py entries --kind receivable contas-a-receber.xls -o entries.csv
    uv run --with openpyxl scripts/conta-azul-to-entries-csv.py statement extrato.xls --out-dir out/

`entries` reads a "visão contas a pagar/receber" export (an .xlsx inside, whatever its extension) and writes the
entries file of `POST /companies/{id}/entries/import`: one row per (category x cost center) piece of an installment,
with the payment of the installment shared among the pieces. `statement` reads the "extrato financeiro" export and
writes the own-account transfers, the opening balances and the last running balance of every account, to compare
with `GET /balances`. Money is handled as whole cents, never as a float.

Nothing is sent anywhere: the files are read and written locally.
"""

import argparse
import csv
import hashlib
import io
import re
import sys
import zipfile
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from datetime import date, datetime
from decimal import Decimal, InvalidOperation
from pathlib import Path

ENTRY_COLUMNS = [
    "reference", "kind", "description", "party_name", "category", "cost_center",
    "competence_date", "due_date", "amount", "settled_on", "applied", "interest", "fine",
    "discount", "bank_account",
]
LOST = "Perdido/Desconsiderado"
TRANSFER_ORIGIN = "Transferência"
OPENING_ORIGIN = "Saldo Conta Bancária"
# The values of the categories and cost centers carry the sign of the cash: a payable's are negative.
KINDS = {
    "payable": {
        "party": "Nome do fornecedor",
        "paid": "Valor pago da parcela (R$)",
        "sign": -1,
    },
    "receivable": {
        "party": "Nome do cliente",
        "paid": "Valor recebido da parcela (R$)",
        "sign": 1,
    },
}
TRANSFER_PAIR_DAYS = 3


class ExportError(Exception):
    """The export is not what the script expects: the message says what to look at."""


def cents(value, what: str) -> int:
    """Whole cents of a cell: a number or text with at most two decimals; blank is zero."""
    if value is None or value == "" or value == "-":
        return 0
    try:
        amount = Decimal(str(value).strip().replace(",", ".")) if isinstance(value, str) else Decimal(repr(value))
    except InvalidOperation as error:
        raise ExportError(f"{what}: {value!r} is not a number") from error
    scaled = amount * 100
    if scaled != scaled.to_integral_value():
        raise ExportError(f"{what}: {value!r} has more than two decimals")
    return int(scaled)


def money(total_cents: int) -> str:
    sign = "-" if total_cents < 0 else ""
    whole, fraction = divmod(abs(total_cents), 100)
    return f"{sign}{whole}.{fraction:02d}"


def iso_date(value, what: str) -> str:
    if value is None or value == "":
        return ""
    if isinstance(value, (datetime, date)):
        return value.strftime("%Y-%m-%d")
    text = str(value).strip()
    if re.fullmatch(r"\d{2}/\d{2}/\d{4}", text):
        day, month, year = text.split("/")
        return f"{year}-{month}-{day}"
    if re.fullmatch(r"\d{4}-\d{2}-\d{2}", text):
        return text
    raise ExportError(f"{what}: {value!r} is not a date (dd/mm/yyyy)")


def days_between(first: str, second: str) -> int:
    return (date.fromisoformat(second) - date.fromisoformat(first)).days


def one_line(text, limit: int) -> str:
    cleaned = re.sub(r"\s+", " ", "".join(c for c in str(text or "") if c.isprintable() or c.isspace())).strip()
    return cleaned[:limit].rstrip()


def read_sheet(path: Path):
    """The header and the rows of the first sheet, whatever the extension says the file is."""
    import openpyxl  # imported here so `--help` works without it

    if not zipfile.is_zipfile(path):
        raise ExportError(f"{path} is not an .xlsx workbook (the exports are, even when named .xls)")
    # From memory, because openpyxl judges a file by its extension and these are named .xls.
    workbook = openpyxl.load_workbook(io.BytesIO(path.read_bytes()), read_only=True, data_only=True)
    rows = workbook.worksheets[0].iter_rows(values_only=True)
    header = next(rows, None)
    if header is None:
        raise ExportError(f"{path} has no rows")
    return list(header), [row for row in rows if any(cell is not None for cell in row)]


def column_index(header: list, name: str) -> int:
    try:
        return header.index(name)
    except ValueError as error:
        raise ExportError(f"the export has no column {name!r}") from error


@dataclass
class Piece:
    category: str
    cost_center: str
    cents: int


def pieces_of(header: list, row: tuple, sign: int) -> list[Piece]:
    """The (category, cost center) pieces of a row: the columns come in blocks, `Categoria n` with its value
    and then its cost centers with theirs, up to the next `Categoria`. What a block's cost centers do not
    cover stays in the category with no cost center."""
    blocks: list[tuple[str, int, list[tuple[str, int]]]] = []
    pending_center = None
    for index, name in enumerate(header):
        cell = row[index] if index < len(row) else None
        if re.fullmatch(r"Categoria \d+", str(name)):
            blocks.append((one_line(cell, 200), 0, []))
            pending_center = None
        elif re.fullmatch(r"Valor na Categoria \d+", str(name)) and blocks:
            category, _, centers = blocks[-1]
            blocks[-1] = (category, sign * cents(cell, str(name)), centers)
        elif re.fullmatch(r"Centro de Custo \d+", str(name)) and blocks:
            pending_center = one_line(cell, 200)
        elif re.fullmatch(r"Valor no Centro de Custo \d+", str(name)) and blocks:
            if pending_center:
                blocks[-1][2].append((pending_center, sign * cents(cell, str(name))))
            pending_center = None
    pieces: list[Piece] = []
    for category, value, centers in blocks:
        if not category:
            continue
        if value < 0 or any(amount < 0 for _, amount in centers):
            raise ExportError(
                "a category has a negative value (a deduction inside the installment), which an entry cannot hold"
            )
        covered = sum(amount for _, amount in centers)
        if covered > value:
            raise ExportError(f"the cost centers of {category!r} add up to more than its value")
        pieces.extend(Piece(category, center, amount) for center, amount in centers if amount > 0)
        if value - covered > 0:
            pieces.append(Piece(category, "", value - covered))
    return pieces


def distribute(total: int, caps: list[int]) -> list[int]:
    """`total` cents shared among pieces in proportion to `caps`, none above its own cap, the rounding
    remainder on the last."""
    whole = sum(caps)
    if total == 0 or whole == 0:
        return [0] * len(caps)
    if total > whole:
        raise ExportError("more was paid than the installment is worth")
    shares = [total * cap // whole for cap in caps]
    shares[-1] = total - sum(shares[:-1])
    # Flooring can leave the last above its own cap: move the excess to a piece that has room.
    index = len(shares) - 1
    while index > 0 and shares[index] > caps[index]:
        excess = shares[index] - caps[index]
        shares[index] = caps[index]
        for other in range(index):
            room = min(excess, caps[other] - shares[other])
            shares[other] += room
            excess -= room
        index -= 1
    return shares


def reference_of(kind: str, fields: list, occurrence: int, piece: int) -> str:
    """Stable for a row that did not change: only what is fixed once an installment is entered (not what
    was paid) is hashed, so a later export of the same installment keeps its reference."""
    digest = hashlib.sha256("\x1f".join(str(field) for field in fields).encode()).hexdigest()[:20]
    return f"ca-{kind[:3]}-{digest}-{occurrence}-{piece}"


@dataclass
class EntriesReport:
    rows_in: int = 0
    entries_out: int = 0
    split_rows: int = 0
    lost: int = 0
    zero: int = 0
    settled: int = 0
    open: int = 0
    problems: list[str] = field(default_factory=list)


def convert_entries(path: Path, kind: str, categories: dict[tuple[str, str], str]) -> tuple[list[dict], EntriesReport]:
    names = KINDS[kind]
    header, rows = read_sheet(path)
    at = {name: column_index(header, name) for name in (
        names["party"], "Data de competência", "Data de vencimento", "Descrição", "Situação",
        "Valor original da parcela (R$)", names["paid"], "Juros realizado (R$)", "Multa realizado (R$)",
        "Desconto realizado (R$)", "Conta bancária", "Data do último pagamento",
    )}
    report = EntriesReport()
    out: list[dict] = []
    seen: Counter = Counter()
    for number, row in enumerate(rows, start=2):
        report.rows_in += 1
        cell = lambda name: row[at[name]] if at[name] < len(row) else None  # noqa: E731
        status = one_line(cell("Situação"), 40)
        if status == LOST:
            report.lost += 1
            continue
        try:
            original = cents(cell("Valor original da parcela (R$)"), "Valor original da parcela")
            if original == 0:
                # An installment of nothing (a fee netted out) has no entry: amounts are above zero.
                report.zero += 1
                continue
            pieces = pieces_of(header, row, names["sign"])
            if not pieces or sum(piece.cents for piece in pieces) != original:
                raise ExportError("the categories do not add up to the value of the installment")
            due = iso_date(cell("Data de vencimento"), "Data de vencimento")
            competence = iso_date(cell("Data de competência"), "Data de competência") or due
            applied = cents(cell(names["paid"]), names["paid"])
            interest = cents(cell("Juros realizado (R$)"), "Juros realizado")
            fine = cents(cell("Multa realizado (R$)"), "Multa realizado")
            discount = cents(cell("Desconto realizado (R$)"), "Desconto realizado")
            settled_on = iso_date(cell("Data do último pagamento"), "Data do último pagamento")
            account = one_line(cell("Conta bancária"), 200)
            if applied > 0 and (not settled_on or not account):
                raise ExportError("paid, but without a payment day or a bank account")
            party = one_line(cell(names["party"]), 200)
            description = one_line(cell("Descrição"), 200)
            shares = distribute(applied, [piece.cents for piece in pieces])
            interests = distribute_free(interest, [piece.cents for piece in pieces])
            fines = distribute_free(fine, [piece.cents for piece in pieces])
            discounts = distribute_free(discount, [piece.cents for piece in pieces])
        except ExportError as error:
            report.problems.append(f"row {number}: {error}")
            continue
        fixed = [kind, party, competence, due, description, original,
                 *[(p.category, p.cost_center, p.cents) for p in pieces]]
        seen[tuple(map(str, fixed))] += 1
        occurrence = seen[tuple(map(str, fixed))]
        if len(pieces) > 1:
            report.split_rows += 1
        if applied > 0:
            report.settled += 1
        else:
            report.open += 1
        for index, piece in enumerate(pieces):
            record = {
                "reference": reference_of(kind, fixed, occurrence, index),
                "kind": kind,
                "description": description or piece.category,
                "party_name": party,
                "category": categories.get((kind, piece.category), piece.category),
                "cost_center": piece.cost_center,
                "competence_date": competence,
                "due_date": due,
                "amount": money(piece.cents),
                "settled_on": "",
                "applied": "",
                "interest": "",
                "fine": "",
                "discount": "",
                "bank_account": "",
            }
            if shares[index] > 0:
                record.update(
                    settled_on=settled_on,
                    applied=money(shares[index]),
                    interest=money(interests[index]) if interests[index] else "",
                    fine=money(fines[index]) if fines[index] else "",
                    discount=money(discounts[index]) if discounts[index] else "",
                    bank_account=account,
                )
            out.append(record)
            report.entries_out += 1
    return out, report


def distribute_free(total: int, caps: list[int]) -> list[int]:
    """Like `distribute`, for amounts with no ceiling (interest, fine, discount): in proportion, the
    remainder on the last."""
    whole = sum(caps)
    if total == 0 or whole == 0:
        return [0] * len(caps)
    shares = [total * cap // whole for cap in caps]
    shares[-1] = total - sum(shares[:-1])
    return shares


def load_categories(path: Path | None) -> dict[tuple[str, str], str]:
    """`kind;name;path` rows that say which category of the target a name of the export is, for the names
    the target has under more than one parent."""
    if path is None:
        return {}
    with path.open(encoding="utf-8-sig", newline="") as handle:
        sample = handle.readline()
        handle.seek(0)
        delimiter = ";" if sample.count(";") > sample.count(",") else ","
        return {(row["kind"].strip(), row["name"].strip()): row["path"].strip()
                for row in csv.DictReader(handle, delimiter=delimiter)}


def write_csv(path: Path, columns: list[str], rows: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns, delimiter=";", lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


@dataclass
class StatementReport:
    rows_in: int = 0
    transfers: int = 0
    unpaired: list[str] = field(default_factory=list)
    openings: int = 0
    excluded: Counter = field(default_factory=Counter)
    not_in_date_order: bool = False


def convert_statement(path: Path, known: set[str] | None):
    header, rows = read_sheet(path)
    at = {name: column_index(header, name) for name in (
        "Data movimento", "Descrição", "Tipo", "Origem do lançamento", "Conta bancária", "Valor (R$)", "Saldo conta (R$)",
    )}
    report = StatementReport()
    outflows: dict[int, list[tuple[str, int, str]]] = defaultdict(list)
    inflows: dict[int, list[tuple[str, int, str]]] = defaultdict(list)
    openings: list[dict] = []
    last_balance: dict[str, tuple[str, int, int]] = {}
    previous = ""
    for number, row in enumerate(rows, start=2):
        report.rows_in += 1
        cell = lambda name: row[at[name]] if at[name] < len(row) else None  # noqa: E731
        account = one_line(cell("Conta bancária"), 200)
        if known is not None and account not in known:
            report.excluded[account] += 1
            continue
        day = iso_date(cell("Data movimento"), "Data movimento")
        if day < previous:
            report.not_in_date_order = True
        previous = max(previous, day)
        value = cents(cell("Valor (R$)"), "Valor")
        balance = cents(cell("Saldo conta (R$)"), "Saldo conta")
        origin = one_line(cell("Origem do lançamento"), 80)
        kept = last_balance.get(account)
        if kept is None or (day, number) >= (kept[0], kept[2]):
            last_balance[account] = (day, balance, number)
        if origin == OPENING_ORIGIN:
            openings.append({"account": account, "on": day, "balance": money(value)})
        elif origin == TRANSFER_ORIGIN:
            side = outflows if one_line(cell("Tipo"), 20) == "Despesa" else inflows
            side[abs(value)].append((day, number, account))
    transfers: list[dict] = []
    for value in sorted(set(outflows) | set(inflows)):
        waiting = list(inflows.get(value, []))
        for day, out_number, source in outflows.get(value, []):
            # The money leaves one day and may arrive a few days later: pair it with the nearest arrival
            # of the same value.
            near = [item for item in waiting if abs(days_between(day, item[0])) <= TRANSFER_PAIR_DAYS]
            if not near:
                report.unpaired.append(f"row {out_number} (out, {day}, {money(value)})")
                continue
            arrival = min(near, key=lambda item: (abs(days_between(day, item[0])), item[1]))
            waiting.remove(arrival)
            transfers.append({
                "reference": f"ca-tr-{day}-{value}-{out_number}",
                "occurred_on": day,
                "from_account": source,
                "to_account": arrival[2],
                "amount": money(value),
                "note": f"extract rows {out_number}/{arrival[1]}",
            })
        report.unpaired.extend(f"row {number} (in, {day}, {money(value)})" for day, number, _ in waiting)
    balances = [{"account": account, "last_movement_on": day, "balance": money(balance)}
                for account, (day, balance, _) in sorted(last_balance.items())]
    report.transfers, report.openings = len(transfers), len(openings)
    return transfers, openings, balances, report


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    entries = commands.add_parser("entries", help="contas a pagar/receber -> entries file")
    entries.add_argument("input", type=Path)
    entries.add_argument("--kind", choices=sorted(KINDS), required=True)
    entries.add_argument("-o", "--output", type=Path, required=True)
    entries.add_argument("--categories", type=Path,
                         help="kind;name;path rows for names the target has under more than one parent")
    statement = commands.add_parser("statement", help="extrato -> transfers, opening balances, last balances")
    statement.add_argument("input", type=Path)
    statement.add_argument("--out-dir", type=Path, required=True)
    statement.add_argument("--known-account", action="append",
                           help="a bank account of the target (repeat); rows of any other are reported and left out")
    args = parser.parse_args(argv)
    try:
        if args.command == "entries":
            records, report = convert_entries(args.input, args.kind, load_categories(args.categories))
            write_csv(args.output, ENTRY_COLUMNS, records)
            print(f"rows in: {report.rows_in}; entries out: {report.entries_out}; rows split in pieces: "
                  f"{report.split_rows}; settled: {report.settled}; open: {report.open}; lost, left out: {report.lost}; "
                  f"worth zero, left out: {report.zero}", file=sys.stderr)
            for message, count in Counter(p.split(": ", 1)[1] for p in report.problems).most_common():
                rows = [p.split(":")[0] for p in report.problems if p.endswith(message)][:5]
                print(f"problem x{count}: {message} ({', '.join(rows)}...)", file=sys.stderr)
            return 1 if report.problems else 0
        known = set(args.known_account) if args.known_account else None
        transfers, openings, balances, report = convert_statement(args.input, known)
        write_csv(args.out_dir / "transfers.csv", ["reference", "occurred_on", "from_account", "to_account", "amount", "note"], transfers)
        write_csv(args.out_dir / "opening-balances.csv", ["account", "on", "balance"], openings)
        write_csv(args.out_dir / "last-balances.csv", ["account", "last_movement_on", "balance"], balances)
        print(f"rows in: {report.rows_in}; transfers: {report.transfers}; opening balances: {report.openings}; "
              f"unpaired transfer rows: {len(report.unpaired)}", file=sys.stderr)
        for account, count in sorted(report.excluded.items()):
            print(f"left out: account {account!r} is not known ({count} rows)", file=sys.stderr)
        for line in report.unpaired:
            print(f"unpaired: {line}", file=sys.stderr)
        if report.not_in_date_order:
            print("note: the rows are not in date order, the last balance is taken by date and row", file=sys.stderr)
        return 0
    except ExportError as error:
        print(f"error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
