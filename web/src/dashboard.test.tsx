import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Balances, CashFlow, CashFlowPeriod, OverdueReport, SalesReport } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt, type Reply } from "./testSupport";

const account = (id: string, name: string) => ({
  id,
  company_id: "c1",
  kind: "checking" as const,
  name,
  bank_code: null,
  branch: null,
  number: null,
  active: true,
});

const balances: Balances = {
  on: "2026-11-15",
  total: "48720.35",
  accounts: [
    { account: account("a1", "Conta Itaú"), opening: null, received: "0", paid: "0", transfers_in: "0", transfers_out: "0", balance: "40000" },
    { account: account("a2", "Caixa da loja"), opening: null, received: "0", paid: "0", transfers_in: "0", transfers_out: "0", balance: "8720.35" },
  ],
};

const period = (start: string, extra: Partial<CashFlowPeriod> = {}): CashFlowPeriod => ({
  start,
  end: start,
  received: "0",
  paid: "0",
  transfers_in: "0",
  transfers_out: "0",
  net: "0",
  new_opening_balances: "0",
  closing_balance: "0",
  to_receive: "0",
  to_pay: "0",
  projected_balance: null,
  ...extra,
});

// The forecast: from today through 60 days after it, every account.
const forecast: CashFlow = {
  from: "2026-11-15",
  to: "2027-01-14",
  granularity: "day",
  account_id: null,
  opening_balance: "48720.35",
  past_due: { to_receive: "8420", to_pay: "3150.75" },
  periods: [
    period("2026-11-15", { to_receive: "500", to_pay: "200" }),
    period("2026-11-18", { to_receive: "1000", to_pay: "300" }),
    period("2026-12-01", { to_receive: "2000", to_pay: "400" }),
    period("2027-01-10", { to_receive: "4000", to_pay: "800" }),
  ],
};

// What was received and paid in the month so far.
const month: CashFlow = {
  ...forecast,
  from: "2026-11-01",
  to: "2026-11-30",
  periods: [period("2026-11-02", { received: "1000", paid: "400" }), period("2026-11-10", { received: "500.5", paid: "100" })],
};

const overdue = (kind: "receivable" | "payable"): OverdueReport => ({
  on: "2026-11-15",
  items: [
    { entry_id: "e1", kind, description: kind === "receivable" ? "Pedido 1042" : "Nota 8891", party_id: null, category_id: "c", installment: 1, due_date: "2026-11-02", amount: "100", settled: "0", open: "100", days_late: 13 },
  ],
  receivable: kind === "receivable" ? "8420" : "0",
  payable: kind === "payable" ? "3150.75" : "0",
  count: kind === "receivable" ? 3 : 2,
});

const sales = (by: "product" | "customer"): SalesReport => ({
  from: "2026-11-01",
  to: "2026-11-30",
  by,
  sales: 4,
  total: "1000",
  rows: [{ key: by === "product" ? "pr1" : "p1", sales: 3, net: "850", share: "85", cumulative: "85", class: "A" }],
});

const answer =
  (reply: (url: string) => Reply | undefined = () => undefined) =>
  (url: string): Reply => {
    const special = reply(url);
    if (special) return special;
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.includes("/balances")) return { status: 200, body: balances };
    if (url.includes("/statement-lines")) {
      return { status: 200, body: { items: [{ id: "l1" }, { id: "l2" }, { id: "l3" }], next_cursor: null } };
    }
    if (url.includes("/reports/overdue")) return { status: 200, body: overdue(url.includes("kind=payable") ? "payable" : "receivable") };
    if (url.includes("/cash-flow")) return { status: 200, body: url.includes("to=2027-01-14") ? forecast : month };
    if (url.includes("/sales-reports")) return { status: 200, body: sales(url.includes("by=product") ? "product" : "customer") };
    if (url.includes("/parties")) return { status: 200, body: { items: [{ id: "p1", name: "Mercado Boa Compra" }], next_cursor: null } };
    if (url.includes("/products")) return { status: 200, body: { items: [{ id: "pr1", name: "Café 500g" }], next_cursor: null } };
    return { status: 404, body: { error: "not found" } };
  };

beforeEach(() => {
  vi.restoreAllMocks();
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(2026, 10, 15, 12));
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("dashboard", () => {
  it("shows the balance of every account as of today, and how many statement lines wait to be reconciled", async () => {
    const fetchMock = mockApi(answer());
    renderAt("/companies/c1");

    const card = within(await screen.findByRole("region", { name: "Saldo em caixa" }));
    expect(await card.findByText("R$ 48.720,35")).toBeInTheDocument();
    expect(card.getByText(/Em 15\/11\/2026/)).toBeInTheDocument();
    const accounts = within(card.getByRole("list", { name: "Saldo por conta" }));
    expect(accounts.getByText("Conta Itaú").closest("li")).toHaveTextContent("R$ 40.000,00");
    expect(accounts.getByText("Caixa da loja").closest("li")).toHaveTextContent("R$ 8.720,35");
    expect(card.getByRole("link", { name: "3 linhas de extrato para conciliar" })).toHaveAttribute("href", "/companies/c1/reconciliation");
    expect(fetchMock.mock.calls.map(([url]) => String(url))).toContain("/api/companies/c1/balances?on=2026-11-15");
  });

  it("keeps receivable and payable as two mirrored blocks with what is overdue and what falls due soon", async () => {
    mockApi(answer());
    renderAt("/companies/c1");

    const receivable = within(await screen.findByRole("region", { name: "A receber" }));
    expect(await receivable.findByText("Recebimentos em atraso")).toBeInTheDocument();
    expect(receivable.getByText("Recebimentos em atraso").closest(".stat-tile")).toHaveTextContent("R$ 8.420,00");
    expect(receivable.getByText("3 títulos vencidos")).toBeInTheDocument();
    expect(await receivable.findByText("Vence hoje")).toBeInTheDocument();
    expect(receivable.getByText("Vence hoje").closest(".stat-tile")).toHaveTextContent("R$ 500,00");
    expect(receivable.getByText("Próximos 7 dias").closest(".stat-tile")).toHaveTextContent("R$ 1.000,00");
    expect(receivable.getByText("8 a 30 dias").closest(".stat-tile")).toHaveTextContent("R$ 2.000,00");
    expect(receivable.getByText(/Pedido 1042/)).toBeInTheDocument();

    const payable = within(screen.getByRole("region", { name: "A pagar" }));
    expect(await payable.findByText("Pagamentos em atraso")).toBeInTheDocument();
    expect(payable.getByText("Pagamentos em atraso").closest(".stat-tile")).toHaveTextContent("R$ 3.150,75");
    expect(payable.getByText("Vence hoje").closest(".stat-tile")).toHaveTextContent("R$ 200,00");
    // Each block starts an entry from where the person is.
    expect(payable.getByRole("button", { name: "Novo lançamento" })).toBeInTheDocument();
  });

  it("offers the due ranges as a table, with what is overdue named in words", async () => {
    mockApi(answer());
    renderAt("/companies/c1");
    await screen.findByRole("region", { name: "Saldo em caixa" });

    // The first chart of the page is the due ranges.
    await userEvent.click((await screen.findAllByRole("button", { name: "Ver como tabela" }))[0]);
    const table = await screen.findByRole("table", { name: "Contas por vencimento" });
    expect(within(table).getByRole("cell", { name: "Vencidas" })).toBeInTheDocument();
    expect(within(table).getByRole("row", { name: /Vencidas/ })).toHaveTextContent("R$ 8.420,00");
    expect(within(table).getByRole("row", { name: /31 a 60 dias/ })).toHaveTextContent("R$ 4.000,00");
  });

  it("adds up what came in and went out in the chosen period, and asks again for another", async () => {
    const fetchMock = mockApi(answer());
    renderAt("/companies/c1");

    expect(await screen.findByText("Resultado: R$ 1.000,50")).toBeInTheDocument();
    expect(screen.getByText(/R\$ 1\.500,50 recebidos, R\$ 500,00 pagos/)).toBeInTheDocument();
    const urls = () => fetchMock.mock.calls.map(([url]) => String(url));
    expect(urls()).toContain("/api/companies/c1/cash-flow?from=2026-11-01&to=2026-11-30&granularity=day");

    await userEvent.selectOptions(within(screen.getByRole("group", { name: "Filtro de período" })).getByLabelText("Período"), "last-month");
    await screen.findByRole("heading", { name: "Entradas e saídas" });
    expect(urls().some((url) => url.includes("cash-flow?from=2026-10-01&to=2026-10-31"))).toBe(true);
  });

  it("names the top products and customers of the period", async () => {
    mockApi(answer());
    renderAt("/companies/c1");

    const products = within(await screen.findByRole("table", { name: "Produtos mais vendidos" }));
    expect(await products.findByText("Café 500g")).toBeInTheDocument();
    expect(products.getByText("R$ 850,00")).toBeInTheDocument();
    const customers = within(screen.getByRole("table", { name: "Clientes que mais compraram" }));
    expect(await customers.findByText("Mercado Boa Compra")).toBeInTheDocument();
    expect(screen.getByText("Total vendido", { selector: ".stat-label" }).closest(".stat-tile")).toHaveTextContent("R$ 1.000,00");
    expect(screen.getByText("4 vendas concluídas")).toBeInTheDocument();
  });

  it("says which card failed and still draws the others", async () => {
    mockApi(answer((url) => (url.includes("/sales-reports") ? { status: 500, body: { error: "boom" } } : undefined)));
    renderAt("/companies/c1");

    const sales = within(await screen.findByRole("region", { name: "Vendas do período" }));
    expect((await sales.findAllByRole("alert"))[0]).toBeInTheDocument();
    expect(await screen.findByText("R$ 48.720,35")).toBeInTheDocument();
    expect(await screen.findByText("Resultado: R$ 1.000,50")).toBeInTheDocument();
  });

  it("stamps when it was read and reads everything again on request", async () => {
    const fetchMock = mockApi(answer());
    renderAt("/companies/c1");
    expect(await screen.findByText(/Atualizado em 15\/11\/2026/)).toBeInTheDocument();
    await screen.findByText("R$ 48.720,35");
    const balanceReads = () => fetchMock.mock.calls.filter(([url]) => String(url).includes("/balances")).length;
    const before = balanceReads();

    await userEvent.click(screen.getByRole("button", { name: "Atualizar" }));

    await vi.waitFor(() => expect(balanceReads()).toBe(before + 1));
  });

  it("offers to start an entry only to a role that may write the finances", async () => {
    mockApi(
      answer((url) =>
        url === "/api/auth/me"
          ? {
              status: 200,
              body: {
                ...profile,
                companies: [{ id: "c1", name: "Padaria", role: "viewer", permissions: { ...OWNER_PERMISSIONS, finance: "read" } }],
              },
            }
          : undefined,
      ),
    );
    renderAt("/companies/c1");
    await screen.findByRole("region", { name: "A receber" });

    expect(screen.queryByRole("button", { name: "Novo lançamento" })).not.toBeInTheDocument();
  });
});
