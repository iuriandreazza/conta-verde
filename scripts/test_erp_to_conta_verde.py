"""Tests of erp-to-conta-verde.py on small made-up registers, never on real data.

    uv run python -m unittest scripts/test_erp_to_conta_verde.py
"""

import importlib.util
import sys
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("erp-to-conta-verde.py")
spec = importlib.util.spec_from_file_location("erp_to_conta_verde", SCRIPT)
tool = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = tool
spec.loader.exec_module(tool)


def category(id, name, parent=None, dre=None):
    return {"id": id, "nome": name, "categoria_pai": parent, "entrada_dre": dre}


def installment(id, total, paid=None, shares=None, status="QUITADO", payments=None):
    return {
        "id": id, "status": status, "descricao": "  A  description ", "data_vencimento": "2024-02-10",
        "valor_pago": paid if paid is not None else (total if status == "QUITADO" else 0), "nao_pago": 0 if status == "QUITADO" else total,
        "evento": {"tipo": "DESPESA", "data_competencia": "2024-02-01", "rateio": shares or []},
        "baixas": payments if payments is not None else [{
            "data_pagamento": "2024-02-12", "conta_financeira": {"id": "acc"},
            "valor_composicao": {"valor_bruto": total, "juros": 0, "multa": 0, "desconto": 0, "taxa": 0},
        }],
    }


class MoneyTest(unittest.TestCase):
    def test_cents_are_exact_and_formatted_with_two_decimals(self):
        self.assertEqual(tool.to_cents(70.87), 7087)
        self.assertEqual(tool.to_cents("0.1") + tool.to_cents("0.2"), 30)
        self.assertEqual(tool.money(5), "0.05")
        self.assertEqual(tool.money(-1205), "-12.05")

    def test_a_split_adds_up_and_the_last_share_takes_the_rest(self):
        shares = tool.split_cents(1000, [1, 1, 1])
        self.assertEqual(shares, [333, 333, 334])
        self.assertEqual(tool.split_cents(10000, [7000, 3000]), [7000, 3000])
        self.assertEqual(sum(tool.split_cents(1994_46, [5, 3, 11])), 1994_46)


class DocumentTest(unittest.TestCase):
    def test_check_digits(self):
        self.assertTrue(tool.valid_cpf("52998224725"))
        self.assertFalse(tool.valid_cpf("52998224726"))
        self.assertFalse(tool.valid_cpf("11111111111"))
        self.assertTrue(tool.valid_cnpj("11222333000181"))
        self.assertFalse(tool.valid_cnpj("11222333000182"))
        self.assertFalse(tool.valid_cnpj("ESPANHA"))


class CategoriesTest(unittest.TestCase):
    def test_a_root_takes_the_group_of_its_descendants_and_the_tree_keeps_its_depth(self):
        report = tool.Report()
        expense = [
            category("r", "4.05 Administrative"),
            category("m", "Office", "r", "DESPESAS_ADMINISTRATIVAS"),
            category("g", "Paper", "m", "DESPESAS_ADMINISTRATIVAS"),
        ]
        registers, paths = tool.plan_categories([], expense, report)
        self.assertEqual(registers[0]["dre_group"], "operating_expenses")
        self.assertEqual([r["parent"] for r in registers], [None, "4.05 Administrative", "4.05 Administrative > Office"])
        self.assertEqual(paths["g"], "4.05 Administrative > Office > Paper")

    def test_an_income_root_and_an_expense_root_never_share_a_name(self):
        report = tool.Report()
        registers, paths = tool.plan_categories([category("i", "Fees")], [category("e", "Fees")], report)
        self.assertEqual(len({r["name"].casefold() for r in registers}), 2)
        self.assertEqual(report.counts["category with a repeated name among its siblings, renamed"], 1)

    def test_a_groups_name_that_matches_nothing_is_said(self):
        report = tool.Report()
        tool.plan_categories([category("i", "Fees")], [], report, {"Fess": ["gross_revenue", "operating"]})
        self.assertEqual(report.counts["--groups name that is no root category"], 1)

    def test_a_root_the_old_system_leaves_ungrouped_goes_outside_the_result_unless_told(self):
        income = [category("a", "Cashback")]
        report = tool.Report()
        registers, _ = tool.plan_categories(income, [], report)
        self.assertEqual(registers[0]["dre_group"], "outside_result")
        self.assertEqual(report.counts["root category without a group, put outside the result"], 1)
        registers, _ = tool.plan_categories(income, [], tool.Report(), {"Cashback": ["financial_result", "operating"]})
        self.assertEqual(registers[0]["dre_group"], "financial_result")


class EntriesTest(unittest.TestCase):
    PATHS = {"c1": "Root > One", "c2": "Root > Two"}
    CENTERS = {"k1": "Center"}
    ACCOUNTS = {"acc": "Main"}

    def plan(self, detail, party=None):
        report = tool.Report()
        item = {"id": detail["id"], "fornecedor": party or {"id": "p", "nome": "Supplier"}}
        rows = tool.plan_entries([item], {detail["id"]: detail}, {"p": ("11222333000181", "Supplier")}, self.PATHS, self.CENTERS, self.ACCOUNTS, report)
        return rows, report

    def test_one_category_makes_one_settled_row(self):
        rows, _ = self.plan(installment("i1", 100.5, shares=[{"id_categoria": "c1", "valor": 100.5, "rateio_centro_custo": []}]))
        self.assertEqual(len(rows), 1)
        row = rows[0]
        self.assertEqual((row["reference"], row["kind"], row["amount"], row["applied"]), ("i1", "payable", "100.50", "100.50"))
        self.assertEqual((row["settled_on"], row["bank_account"], row["party_document"]), ("2024-02-12", "Main", "11222333000181"))
        self.assertEqual(row["description"], "A description")

    def test_pieces_by_category_and_cost_center_add_up_to_the_installment_and_share_the_payment(self):
        shares = [
            {"id_categoria": "c1", "valor": 60, "rateio_centro_custo": [{"id_centro_custo": "k1", "valor": 60}]},
            {"id_categoria": "c2", "valor": 40, "rateio_centro_custo": []},
        ]
        payments = [{"data_pagamento": "2024-02-12", "conta_financeira": {"id": "acc"},
                     "valor_composicao": {"valor_bruto": 100, "juros": 1, "multa": 0, "desconto": 0.5, "taxa": 0}}]
        rows, _ = self.plan(installment("i2", 100, shares=shares, payments=payments))
        self.assertEqual([r["reference"] for r in rows], ["i2:1", "i2:2"])
        self.assertEqual([r["amount"] for r in rows], ["60.00", "40.00"])
        self.assertEqual([r["cost_center"] for r in rows], ["Center", ""])
        self.assertEqual(sum(int(r["interest"].replace(".", "")) for r in rows), 100)
        self.assertEqual(sum(int(r["discount"].replace(".", "")) for r in rows), 50)

    def test_the_split_of_an_event_is_scaled_to_the_installment(self):
        shares = [{"id_categoria": "c1", "valor": 300, "rateio_centro_custo": []}]
        rows, _ = self.plan(installment("i3", 100, shares=shares, status="PENDENTE", payments=[]))
        self.assertEqual((rows[0]["amount"], rows[0]["settled_on"], rows[0]["applied"]), ("100.00", "", ""))

    def test_lost_zero_and_unpaid_without_a_payment_are_left_out_and_said(self):
        for detail in (installment("l", 10, status="PERDIDO"), installment("z", 0, shares=[{"id_categoria": "c1", "valor": 0}]),
                       installment("n", 10, payments=[], shares=[{"id_categoria": "c1", "valor": 10}])):
            rows, report = self.plan(detail)
            self.assertEqual(rows, [])
            self.assertTrue(report.counts)

    def test_a_payment_fee_is_carried_as_a_discount_so_the_cash_agrees(self):
        shares = [{"id_categoria": "c1", "valor": 100, "rateio_centro_custo": []}]
        payments = [{"data_pagamento": "2024-02-12", "conta_financeira": {"id": "acc"},
                     "valor_composicao": {"valor_bruto": 100, "juros": 0, "multa": 0, "desconto": 1, "taxa": 2.5}}]
        rows, _ = self.plan(installment("i5", 100, shares=shares, payments=payments))
        self.assertEqual((rows[0]["applied"], rows[0]["discount"]), ("100.00", "3.50"))

    def test_a_lost_installment_that_was_paid_is_imported_as_paid(self):
        shares = [{"id_categoria": "c1", "valor": 50, "rateio_centro_custo": []}]
        detail = installment("i6", 50, status="PERDIDO", shares=shares)
        detail["valor_pago"], detail["nao_pago"] = 50, 0
        rows, report = self.plan(detail)
        self.assertEqual((len(rows), rows[0]["applied"]), (1, "50.00"))
        self.assertEqual(report.counts["lost installment that was paid in part, imported as paid"], 1)

    def test_two_payments_are_joined_into_one_settlement_on_the_last_day(self):
        shares = [{"id_categoria": "c1", "valor": 100, "rateio_centro_custo": []}]
        pay = lambda day, value: {"data_pagamento": day, "conta_financeira": {"id": "acc"},
                                  "valor_composicao": {"valor_bruto": value, "juros": 0, "multa": 0, "desconto": 0, "taxa": 0}}
        rows, report = self.plan(installment("i4", 100, shares=shares, payments=[pay("2024-02-12", 40), pay("2024-03-01", 60)]))
        self.assertEqual((rows[0]["applied"], rows[0]["settled_on"]), ("100.00", "2024-03-01"))
        self.assertEqual(report.counts["installment paid more than once, joined into one settlement"], 1)


class PartiesTest(unittest.TestCase):
    def test_a_bad_document_or_address_is_dropped_but_the_party_stays(self):
        report = tool.Report()
        people = [{"id": "x", "nome": "Foreign Co", "documento": "ESPANHA", "email": "nope", "perfis": ["Cliente"],
                   "endereco": {"cep": "123", "cidade": "Madrid"}}]
        rows, by_id = tool.plan_parties(people, report)
        self.assertEqual((rows[0]["document"], rows[0]["email"], rows[0]["postal_code"], rows[0]["customer"]), ("", "", "", "sim"))
        self.assertEqual(by_id["x"], ("", "Foreign Co"))

    def test_a_full_address_and_the_roles_are_written(self):
        person = {"id": "y", "nome": "Acme", "documento": "11.222.333/0001-81", "perfis": ["Fornecedor"],
                  "endereco": {"cep": "01310-100", "logradouro": "Av. X", "numero": "1", "cidade": "São Paulo", "estado": "sp"}}
        rows, _ = tool.plan_parties([person], tool.Report())
        self.assertEqual((rows[0]["document"], rows[0]["postal_code"], rows[0]["state"], rows[0]["supplier"], rows[0]["customer"]),
                         ("11222333000181", "01310100", "SP", "sim", "não"))


class RegistersTest(unittest.TestCase):
    def test_transfers_and_opening_balances_use_the_account_names_and_sign(self):
        report = tool.Report()
        accounts = {"a": "One", "b": "Two"}
        transfer = {"id": "t", "valor": 12.3, "data": "2024-01-05", "descricao": "move",
                    "origem": {"conta_financeira": {"id": "a"}}, "destino": {"conta_financeira": {"id": "b"}}}
        self.assertEqual(tool.plan_transfers([transfer], accounts, report)[0],
                         {"from": "One", "to": "Two", "amount": "12.30", "occurred_on": "2024-01-05", "note": "move"})
        openings = [{"tipo": "DESPESA", "id_conta_financeira": "a", "data_competencia": "2021-01-01", "saldo_inicial": 5}]
        self.assertEqual(tool.plan_openings(openings, accounts, {}, report)[0]["balance"], "-5.00")

    def test_an_opening_after_the_first_movement_is_brought_back_to_it(self):
        report = tool.Report()
        openings = [{"tipo": "RECEITA", "id_conta_financeira": "a", "data_competencia": "2022-04-29", "saldo_inicial": 0}]
        moves = tool.first_movement_days(
            [{"bank_account": "One", "settled_on": "2021-12-01"}, {"bank_account": "One", "settled_on": ""}],
            [{"from": "One", "to": "Two", "occurred_on": "2022-01-01"}],
        )
        self.assertEqual(moves, {"One": "2021-12-01", "Two": "2022-01-01"})
        self.assertEqual(tool.plan_openings(openings, {"a": "One"}, moves, report)[0]["on"], "2021-12-01")
        self.assertEqual(report.counts["opening balance moved back to the account's first movement"], 1)


if __name__ == "__main__":
    unittest.main()
