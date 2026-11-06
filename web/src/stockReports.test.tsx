import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { StockMovementSummary, StockValuation } from "./api";
import { mockApi, profile, renderAt, type Reply } from "./testSupport";
import { todayIso } from "./lib/settlement";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const valuation = (patch: Partial<StockValuation> = {}): StockValuation => ({
  on: "2026-11-06",
  total: "300",
  negative: 0,
  rows: [
    { product_id: "pr1", name: "Grão de café 1kg", sku: "grao", unit: "KG", quantity: "10", average_cost: "22.500000", value: "225", share: "75" },
    { product_id: "pr2", name: "Filtro de papel", sku: null, unit: "UN", quantity: "25", average_cost: "3.000000", value: "75", share: "25" },
    { product_id: "pr3", name: "Açúcar", sku: null, unit: "KG", quantity: "-2", average_cost: null, value: "0", share: "0" },
  ],
  ...patch,
});

const summary: StockMovementSummary = {
  from: "2026-11-01",
  to: "2026-11-30",
  items: [
    {
      product_id: "pr1", name: "Grão de café 1kg", sku: "grao", unit: "KG",
      opening_quantity: "5", opening_value: "100.10",
      entered_quantity: "10", entered_value: "150.20",
      exited_quantity: "4", exited_value: "60.04",
      adjusted_quantity: "-1", adjusted_value: "-15",
      closing_quantity: "10", closing_value: "175.26",
    },
    {
      product_id: "pr2", name: "Filtro de papel", sku: null, unit: "UN",
      opening_quantity: "0", opening_value: "0",
      entered_quantity: "20", entered_value: "60",
      exited_quantity: "0", exited_value: "0",
      adjusted_quantity: "2.5", adjusted_value: "7.5",
      closing_quantity: "22.5", closing_value: "67.5",
    },
  ],
};

const calls = (fetchMock: ReturnType<typeof mockApi>, fragment: string) =>
  fetchMock.mock.calls.filter(([url]) => String(url).includes(fragment));

const answer =
  (extra: (url: string) => Reply | undefined = () => undefined) =>
  (url: string): Reply => {
    const special = extra(url);
    if (special) return special;
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.includes("/stock-reports/valuation")) return { status: 200, body: valuation() };
    if (url.includes("/stock-reports/movements")) return { status: 200, body: summary };
    return { status: 404, body: { error: "not found" } };
  };

describe("stock valuation", () => {
  it("shows the total, the products with stock, the rows with average cost and share", async () => {
    const fetchMock = mockApi(answer());
    renderAt("/companies/c1/stock-reports");

    const rows = await screen.findAllByRole("row", { name: /Grão de café 1kg/ });
    const row = rows[rows.length - 1];
    expect(row).toHaveTextContent("10 KG");
    expect(row).toHaveTextContent("R$ 22,50R$ 225,00");
    expect(row).toHaveTextContent("R$ 225,00");
    expect(row).toHaveTextContent("75,00%");
    expect(screen.getByText("R$ 300,00")).toBeInTheDocument();
    // Two of the three products have something on hand.
    expect(screen.getByText("Produtos com saldo").closest("div")).toHaveTextContent("2");
    expect(screen.getAllByRole("row", { name: /Açúcar/ }).at(-1)).toHaveTextContent("—");
    expect(screen.queryByText(/saldo negativo/)).not.toBeInTheDocument();
    expect(calls(fetchMock, "valuation?on=")[0][0]).toBe(`/api/companies/c1/stock-reports/valuation?on=${todayIso()}`);
  });

  it("warns how many products are below zero", async () => {
    mockApi(answer((url) => (url.includes("/valuation") ? { status: 200, body: valuation({ negative: 2 }) } : undefined)));
    renderAt("/companies/c1/stock-reports");
    expect(await screen.findByText(/2 produtos com saldo negativo/)).toBeInTheDocument();
  });

  it("asks again for the day chosen", async () => {
    const fetchMock = mockApi(answer());
    renderAt("/companies/c1/stock-reports");
    await screen.findAllByRole("row", { name: /Grão de café 1kg/ });
    fireEvent.change(screen.getByLabelText("Em"), { target: { value: "2026-10-31" } });
    await vi.waitFor(() => expect(calls(fetchMock, "valuation?on=2026-10-31")).toHaveLength(1));
  });

  it("says so when nothing is in stock, and shows an API error", async () => {
    mockApi(answer((url) => (url.includes("/valuation") ? { status: 200, body: valuation({ total: "0", rows: [] }) } : undefined)));
    const { unmount } = renderAt("/companies/c1/stock-reports");
    expect(await screen.findByText("Nenhum produto com saldo nesse dia.")).toBeInTheDocument();
    unmount();

    mockApi(answer((url) => (url.includes("/valuation") ? { status: 500, body: { error: "internal error" } } : undefined)));
    renderAt("/companies/c1/stock-reports");
    expect(await screen.findByRole("alert")).toHaveTextContent("internal error");
  });
});

describe("stock movements report", () => {
  const open = async () => {
    await userEvent.click(await screen.findByRole("button", { name: "Movimentações do período" }));
  };

  it("shows opening, entries, exits, adjustments and closing per product, with a totals row of the values", async () => {
    const fetchMock = mockApi(answer());
    renderAt("/companies/c1/stock-reports");
    await open();

    const beans = await screen.findByRole("row", { name: /Grão de café 1kg/ });
    expect(within(beans).getAllByRole("cell").map((cell) => cell.textContent)).toEqual([
      "Grão de café 1kggrao",
      "5 KGR$ 100,10",
      "10 KGR$ 150,20",
      "4 KGR$ 60,04",
      "-1 KGR$ -15,00",
      "10 KGR$ 175,26",
    ]);
    expect(screen.getByRole("row", { name: /Filtro de papel/ })).toHaveTextContent("+2,5 UNR$ 7,50");
    const totals = screen.getByRole("row", { name: /Total \(valores\)/ });
    // 100.10, 150.20, 60.04, -15 + 7.5 and 175.26 + 67.5, added exactly.
    expect(within(totals).getAllByRole("cell").map((cell) => cell.textContent)).toEqual([
      "R$ 100,10",
      "R$ 210,20",
      "R$ 60,04",
      "R$ -7,50",
      "R$ 242,76",
    ]);
    expect(calls(fetchMock, "movements?from=")).toHaveLength(1);
  });

  it("shows the server's refusal of a span that is too long", async () => {
    mockApi(
      answer((url) =>
        url.includes("/stock-reports/movements") ? { status: 400, body: { error: "the span is longer than 366 days" } } : undefined,
      ),
    );
    renderAt("/companies/c1/stock-reports");
    await open();
    expect(await screen.findByRole("alert")).toHaveTextContent("the span is longer than 366 days");
  });

  it("says so when nothing moved", async () => {
    mockApi(answer((url) => (url.includes("/stock-reports/movements") ? { status: 200, body: { ...summary, items: [] } } : undefined)));
    renderAt("/companies/c1/stock-reports");
    await open();
    expect(await screen.findByText("Nenhum produto se moveu nesse período.")).toBeInTheDocument();
  });
});
