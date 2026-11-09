import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { CostCenter, Dre, DreLines } from "./api";
import { mockApi, profile, renderAt, type Reply } from "./testSupport";

const lines = (values: Partial<DreLines>): DreLines => ({
  gross_revenue: "0",
  deductions: "0",
  costs: "0",
  operating_expenses: "0",
  financial_result: "0",
  investments: "0",
  outside_result: "0",
  net_revenue: "0",
  gross_profit: "0",
  operating_result: "0",
  net_result: "0",
  ...values,
});

const october = lines({
  gross_revenue: "1470",
  deductions: "-100",
  costs: "-300",
  net_revenue: "1370",
  gross_profit: "1070",
  operating_result: "1070",
  net_result: "1070",
});
const november = lines({
  operating_expenses: "-200",
  financial_result: "-20.5",
  investments: "-400",
  outside_result: "5000",
  operating_result: "-200",
  net_result: "-220.5",
});
const total = lines({
  gross_revenue: "1470",
  deductions: "-100",
  costs: "-300",
  operating_expenses: "-200",
  financial_result: "-20.5",
  investments: "-400",
  outside_result: "5000",
  net_revenue: "1370",
  gross_profit: "1070",
  operating_result: "870",
  net_result: "849.5",
});

const byMonth: Dre = {
  from: "2026-10-01",
  to: "2026-11-30",
  by: "month",
  columns: [
    { key: "2026-10", lines: october },
    { key: "2026-11", lines: november },
  ],
  total,
};
const matriz: CostCenter = { id: "k1", company_id: "c1", name: "Matriz", active: true };
const filial: CostCenter = { id: "k2", company_id: "c1", name: "Filial", active: true };

const answer =
  (reply: (url: string) => Reply | undefined) =>
  (url: string): Reply => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [] } };
    if (url.includes("/cost-centers")) return { status: 200, body: { items: [matriz, filial], next_cursor: null } };
    if (url.includes("/parties")) return { status: 200, body: { items: [], next_cursor: null } };
    return reply(url) ?? { status: 404, body: { error: "not found" } };
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

const open = async () => {
  renderAt("/companies/c1/reports");
  await userEvent.click(await screen.findByRole("button", { name: "DRE" }));
};

describe("the income statement", () => {
  it("asks for this month by month and shows every line with the subtotals the server worked out", async () => {
    const fetchMock = mockApi(answer((url) => (url.includes("/dre?") ? { status: 200, body: byMonth } : undefined)));
    await open();

    const revenue = await screen.findByRole("row", { name: /Receita bruta/ });
    expect(revenue).toHaveTextContent("R$ 1.470,00");
    expect(screen.getByRole("row", { name: /= Receita líquida/ })).toHaveTextContent("R$ 1.370,00");
    expect(screen.getByRole("row", { name: /= Resultado líquido/ })).toHaveTextContent("R$ 849,50");
    expect(screen.getByRole("row", { name: /\(−\) Deduções/ })).toHaveTextContent("R$ -100,00");
    expect(screen.getByRole("columnheader", { name: "out/2026" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "nov/2026" })).toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Total" })).toBeInTheDocument();
    expect(fetchMock.mock.calls.map(([url]) => String(url))).toContain(
      "/api/companies/c1/dre?from=2026-11-01&to=2026-11-30&by=month",
    );
  });

  it("puts investments and loans below the line and says the statement is managerial", async () => {
    mockApi(answer((url) => (url.includes("/dre?") ? { status: 200, body: byMonth } : undefined)));
    await open();

    expect(await screen.findByRole("row", { name: /Investimentos/ })).toHaveClass("below-the-line");
    expect(screen.getByRole("row", { name: /Empréstimos e capital/ })).toHaveTextContent("R$ 5.000,00");
    // The explanation is behind the (i) of the table's title, not loose on the page.
    expect(screen.queryByText(/Demonstração gerencial/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Sobre esta demonstração" }));
    expect(await screen.findByRole("tooltip")).toHaveTextContent(/Demonstração gerencial.*Não substitui a demonstração contábil/);
  });

  it("asks again by cost center and names each column, the entries without one last", async () => {
    const fetchMock = mockApi(
      answer((url) => {
        if (!url.includes("/dre?")) return undefined;
        if (url.includes("by=cost_center")) {
          return {
            status: 200,
            body: {
              ...byMonth,
              by: "cost_center",
              columns: [
                { key: "k1", lines: october },
                { key: null, lines: november },
                { key: "k2", lines: lines({ gross_revenue: "5", net_revenue: "5" }) },
              ],
            } satisfies Dre,
          };
        }
        return { status: 200, body: byMonth };
      }),
    );
    await open();
    await screen.findByRole("row", { name: /Receita bruta/ });

    await userEvent.selectOptions(screen.getByLabelText("Colunas"), "cost_center");
    const headers = await screen.findAllByRole("columnheader");
    expect(headers.map((header) => header.textContent)).toEqual(["Linha", "Filial", "Matriz", "Sem centro de custo", "Total"]);
    expect(fetchMock.mock.calls.map(([url]) => String(url))).toContain(
      "/api/companies/c1/dre?from=2026-11-01&to=2026-11-30&by=cost_center",
    );
  });

  it("narrows to one cost center, or to none", async () => {
    const fetchMock = mockApi(answer((url) => (url.includes("/dre?") ? { status: 200, body: byMonth } : undefined)));
    await open();
    await screen.findByRole("row", { name: /Receita bruta/ });

    await userEvent.selectOptions(screen.getByLabelText("Centro de custo"), "k1");
    await userEvent.selectOptions(screen.getByLabelText("Centro de custo"), "none");
    const urls = fetchMock.mock.calls.map(([url]) => String(url));
    expect(urls).toContain("/api/companies/c1/dre?from=2026-11-01&to=2026-11-30&by=month&cost_center=k1");
    expect(urls).toContain("/api/companies/c1/dre?from=2026-11-01&to=2026-11-30&by=month&cost_center=none");
  });

  it("says when no entry falls in the period instead of drawing an empty statement", async () => {
    mockApi(
      answer((url) =>
        url.includes("/dre?") ? { status: 200, body: { ...byMonth, by: "cost_center", columns: [], total: lines({}) } } : undefined,
      ),
    );
    await open();

    expect(await screen.findByText("Nenhum lançamento com competência nesse período.")).toBeInTheDocument();
    expect(screen.queryByRole("row", { name: /Receita bruta/ })).not.toBeInTheDocument();
  });

  it("says what the server refuses and keeps the controls", async () => {
    mockApi(
      answer((url) =>
        url.includes("/dre?") ? { status: 400, body: { error: "a statement covers at most 24 months" } } : undefined,
      ),
    );
    await open();

    expect(await screen.findByText("a statement covers at most 24 months")).toBeInTheDocument();
    expect(screen.getByLabelText("Colunas")).toBeInTheDocument();
  });

  it("lists the values of the chart in a table for those who cannot see it", async () => {
    mockApi(answer((url) => (url.includes("/dre?") ? { status: 200, body: byMonth } : undefined)));
    await open();
    await screen.findByRole("row", { name: /Receita bruta/ });

    await userEvent.click(screen.getByRole("button", { name: "Ver como tabela" }));
    const chartTable = screen.getByRole("table", { name: "Receita líquida e custos e despesas" });
    expect(within(chartTable).getByRole("row", { name: /out\/2026/ })).toHaveTextContent("R$ 1.370,00");
    expect(within(chartTable).getByRole("row", { name: /nov\/2026/ })).toHaveTextContent("R$ 200,00");
  });
});
