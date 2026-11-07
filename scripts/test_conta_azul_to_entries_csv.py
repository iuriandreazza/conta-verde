"""Tests of conta-azul-to-entries-csv.py, on small workbooks made here, never on real data.

    uv run --with openpyxl python -m unittest scripts/test_conta_azul_to_entries_csv.py
"""

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

import openpyxl

SCRIPT = Path(__file__).with_name("conta-azul-to-entries-csv.py")
spec = importlib.util.spec_from_file_location("conta_azul_to_entries_csv", SCRIPT)
tool = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = tool
spec.loader.exec_module(tool)

PAYABLE_HEADER = [
    "Nome do fornecedor", "Data de competência", "Data de vencimento", "Descrição", "Situação",
    "Valor original da parcela (R$)", "Valor pago da parcela (R$)", "Juros realizado (R$)",
    "Multa realizado (R$)", "Desconto realizado (R$)", "Conta bancária", "Data do último pagamento",
    "Categoria 1", "Valor na Categoria 1", "Centro de Custo 1", "Valor no Centro de Custo 1",
    "Centro de Custo 2", "Valor no Centro de Custo 2",
    "Categoria 2", "Valor na Categoria 2", "Centro de Custo 1", "Valor no Centro de Custo 1",
]


def payable_row(**given):
    row = {
        "Nome do fornecedor": "Papelaria", "Data de competência": "01/03/2026", "Data de vencimento": "10/03/2026",
        "Descrição": "Material", "Situação": "Em aberto", "Valor original da parcela (R$)": 100.0,
        "Valor pago da parcela (R$)": 0, "Juros realizado (R$)": 0, "Multa realizado (R$)": 0,
        "Desconto realizado (R$)": 0, "Conta bancária": None, "Data do último pagamento": None,
    }
    row.update(given)
    return row


def cells(row, blocks):
    """A row in the header's order; `blocks` is [(category, value, [(cost center, value), ...]), ...]."""
    values = [row.get(name) for name in PAYABLE_HEADER[:12]]
    for index, (category, value, centers) in enumerate(blocks):
        values += [category, value]
        padded = (centers + [(None, None), (None, None)])[:2] if index == 0 else (centers + [(None, None)])[:1]
        for center, amount in padded:
            values += [center, amount]
    return values + [None] * (len(PAYABLE_HEADER) - len(values))


def workbook(path: Path, header, rows):
    book = openpyxl.Workbook()
    sheet = book.active
    sheet.append(header)
    for row in rows:
        sheet.append(row)
    book.save(path)


class Converting(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "export.xls"  # an .xlsx, as the real ones are

    def convert(self, rows, kind="payable"):
        workbook(self.path, PAYABLE_HEADER, [cells(row, blocks) for row, blocks in rows])
        return tool.convert_entries(self.path, kind, {})

    def test_a_paid_installment_becomes_one_settled_entry_with_negative_cash_signs_undone(self):
        records, report = self.convert([(
            payable_row(**{"Situação": "Quitado", "Valor original da parcela (R$)": 100.5,
                           "Valor pago da parcela (R$)": 100.5, "Juros realizado (R$)": 1.25,
                           "Conta bancária": "Corrente", "Data do último pagamento": "11/03/2026"}),
            [("Material", -100.5, [("Matriz", -100.5)])],
        )])

        self.assertEqual(report.entries_out, 1)
        self.assertEqual(report.settled, 1)
        record = records[0]
        self.assertEqual(
            {key: record[key] for key in ("amount", "applied", "interest", "settled_on", "bank_account",
                                           "category", "cost_center", "due_date", "competence_date")},
            {"amount": "100.50", "applied": "100.50", "interest": "1.25", "settled_on": "2026-03-11",
             "bank_account": "Corrente", "category": "Material", "cost_center": "Matriz",
             "due_date": "2026-03-10", "competence_date": "2026-03-01"},
        )

    def test_categories_and_cost_centers_split_the_entry_and_its_payment_to_the_cent(self):
        records, report = self.convert([(
            payable_row(**{"Situação": "Quitado", "Valor original da parcela (R$)": 100.0,
                           "Valor pago da parcela (R$)": 100.0, "Desconto realizado (R$)": 0.1,
                           "Conta bancária": "Corrente", "Data do último pagamento": "11/03/2026"}),
            [("Material", -70.0, [("Matriz", -40.0), ("Filial", -30.0)]), ("Frete", -30.0, [])],
        )])

        self.assertEqual(report.split_rows, 1)
        self.assertEqual([r["amount"] for r in records], ["40.00", "30.00", "30.00"])
        self.assertEqual([r["cost_center"] for r in records], ["Matriz", "Filial", ""])
        self.assertEqual([r["applied"] for r in records], ["40.00", "30.00", "30.00"])
        # 10 cents of discount in proportion to 40/30/30 is 4/3/3, and what rounds off goes to the last.
        self.assertEqual([r["discount"] for r in records], ["0.04", "0.03", "0.03"])
        self.assertEqual(len({r["reference"] for r in records}), 3)

    def test_what_the_cost_centers_do_not_cover_stays_in_the_category_without_one(self):
        records, _ = self.convert([(payable_row(), [("Material", -100.0, [("Matriz", -60.0)])])])

        self.assertEqual([(r["cost_center"], r["amount"]) for r in records], [("Matriz", "60.00"), ("", "40.00")])

    def test_an_open_row_has_no_settlement_and_a_lost_or_zero_one_is_left_out(self):
        records, report = self.convert([
            (payable_row(), [("Material", -100.0, [])]),
            (payable_row(**{"Situação": tool.LOST}), [("Material", -100.0, [])]),
            (payable_row(**{"Valor original da parcela (R$)": 0.0}), [("Material", 0.0, [])]),
        ])

        self.assertEqual((report.entries_out, report.lost, report.zero, report.open), (1, 1, 1, 1))
        self.assertEqual((records[0]["settled_on"], records[0]["bank_account"]), ("", ""))

    def test_a_negative_piece_or_a_sum_that_does_not_match_is_a_problem_of_that_row_only(self):
        records, report = self.convert([
            (payable_row(), [("Material", 100.0, [])]),
            (payable_row(), [("Material", -90.0, [])]),
            (payable_row(), [("Material", -100.0, [])]),
        ])

        self.assertEqual(len(records), 1)
        self.assertEqual([p.split(":")[0] for p in report.problems], ["row 2", "row 3"])

    def test_the_reference_survives_a_later_export_where_the_installment_was_paid(self):
        blocks = [("Material", -100.0, [])]
        before, _ = self.convert([(payable_row(), blocks)])
        after, _ = self.convert([(payable_row(**{
            "Situação": "Quitado", "Valor pago da parcela (R$)": 100.0, "Conta bancária": "Corrente",
            "Data do último pagamento": "12/03/2026"}), blocks)])

        self.assertEqual(before[0]["reference"], after[0]["reference"])

    def test_identical_rows_get_references_of_their_own_in_the_order_of_the_file(self):
        blocks = [("Material", -100.0, [])]
        records, _ = self.convert([(payable_row(), blocks), (payable_row(), blocks)])

        self.assertEqual(len({r["reference"] for r in records}), 2)

    def test_amounts_with_more_than_two_decimals_are_refused(self):
        records, report = self.convert([(payable_row(**{"Valor original da parcela (R$)": 10.005}), [("Material", -10.005, [])])])

        self.assertEqual((len(records), len(report.problems)), (0, 1))

    def test_a_category_map_says_which_one_a_repeated_name_is(self):
        workbook(self.path, PAYABLE_HEADER, [cells(payable_row(), [("Juros/Multa", -100.0, [])])])
        records, _ = tool.convert_entries(self.path, "payable", {("payable", "Juros/Multa"): "Despesas > Juros/Multa"})

        self.assertEqual(records[0]["category"], "Despesas > Juros/Multa")

    def test_payment_is_shared_without_any_piece_above_its_own_value(self):
        self.assertEqual(tool.distribute(3, [1, 1, 1]), [1, 1, 1])
        shares = tool.distribute(100, [33, 33, 34])
        self.assertEqual(sum(shares), 100)
        self.assertTrue(all(share <= cap for share, cap in zip(shares, [33, 33, 34])))
        self.assertEqual(tool.distribute(0, [5, 5]), [0, 0])
        with self.assertRaises(tool.ExportError):
            tool.distribute(11, [5, 5])


class Statement(unittest.TestCase):
    HEADER = ["Data movimento", "Descrição", "Tipo", "Origem do lançamento", "Conta bancária", "Valor (R$)", "Saldo conta (R$)"]

    def run_statement(self, rows, known=None):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "extract.xls"
            workbook(path, self.HEADER, rows)
            return tool.convert_statement(path, known)

    def test_transfers_are_paired_across_accounts_and_a_day_apart(self):
        transfers, _, _, report = self.run_statement([
            ["08/03/2026", "t", "Despesa", "Transferência", "Corrente", -50.0, 950.0],
            ["09/03/2026", "t", "Receita", "Transferência", "Reserva", 50.0, 50.0],
            ["10/03/2026", "t", "Despesa", "Transferência", "Corrente", -7.0, 943.0],
        ])

        self.assertEqual(len(transfers), 1)
        self.assertEqual(
            {key: transfers[0][key] for key in ("occurred_on", "from_account", "to_account", "amount")},
            {"occurred_on": "2026-03-08", "from_account": "Corrente", "to_account": "Reserva", "amount": "50.00"},
        )
        self.assertEqual(len(report.unpaired), 1)

    def test_opening_balances_and_the_last_balance_of_every_account(self):
        _, openings, balances, _ = self.run_statement([
            ["01/03/2026", "Saldo inicial", "Receita", "Saldo Conta Bancária", "Corrente", 1000.0, 1000.0],
            ["02/03/2026", "x", "Despesa", "Lançamento Financeiro", "Corrente", -10.5, 989.5],
            ["02/03/2026", "y", "Receita", "Lançamento Financeiro", "Reserva", 5.0, 5.0],
        ])

        self.assertEqual(openings, [{"account": "Corrente", "on": "2026-03-01", "balance": "1000.00"}])
        self.assertEqual(
            balances,
            [{"account": "Corrente", "last_movement_on": "2026-03-02", "balance": "989.50"},
             {"account": "Reserva", "last_movement_on": "2026-03-02", "balance": "5.00"}],
        )

    def test_an_account_the_target_does_not_have_is_reported_and_left_out(self):
        _, _, balances, report = self.run_statement(
            [["02/03/2026", "x", "Despesa", "Lançamento Financeiro", "Outra", -1.0, 9.0],
             ["02/03/2026", "y", "Despesa", "Lançamento Financeiro", "Corrente", -1.0, 9.0]],
            known={"Corrente"},
        )

        self.assertEqual([b["account"] for b in balances], ["Corrente"])
        self.assertEqual(report.excluded, {"Outra": 1})


if __name__ == "__main__":
    unittest.main()
