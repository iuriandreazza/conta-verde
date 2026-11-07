import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { PurchaseOrder, StockPosition } from "./api";
import { mockApi, profile, renderAt } from "./testSupport";

beforeEach(() => {
  vi.restoreAllMocks();
  window.localStorage.clear();
});
afterEach(() => vi.unstubAllGlobals());

const order = {
  id: "o1",
  company_id: "c1",
  number: 7,
  supplier_id: "p1",
  ordered_on: "2026-11-06",
  expected_on: "2026-11-20",
  payment_term_id: null,
  category_id: "k1",
  cost_center_id: null,
  notes: null,
  status: "draft",
  invoice_key: null,
  lines: [{ product_id: "pr1", description: "Café", quantity: "1", unit_price: "10", discount: "0", total: "10" }],
  total: "10",
} as unknown as PurchaseOrder;

const position: StockPosition = {
  product_id: "pr1",
  name: "Grão de café",
  sku: "CAFE",
  unit: "KG",
  active: true,
  minimum_stock: "5",
  quantity: "20",
  value: "120",
  average_cost: "6",
};

describe("choosing the columns of a table", () => {
  it("hides a column of the purchases list, keeps the choice in this browser and restores the usual ones", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/status-counts")) return { status: 200, body: { draft: 1, approved: 0, partially_received: 0, received: 0, cancelled: 0 } };
      if (url.includes("/purchase-orders")) return { status: 200, body: { items: [order], next_cursor: null } };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    const { unmount } = renderAt("/companies/c1/purchases");
    expect(await screen.findByRole("columnheader", { name: "Previsão" })).toBeInTheDocument();

    await userEvent.click(screen.getByText("Colunas"));
    const chooser = within(screen.getByRole("group", { name: "Colunas da tabela" }));
    await userEvent.click(chooser.getByRole("checkbox", { name: "Previsão" }));
    expect(screen.queryByRole("columnheader", { name: "Previsão" })).not.toBeInTheDocument();
    expect(screen.getByRole("columnheader", { name: "Nº da NF" })).toBeInTheDocument();

    // The choice survives a new visit to the page.
    unmount();
    renderAt("/companies/c1/purchases");
    expect(await screen.findByRole("columnheader", { name: "Nº da NF" })).toBeInTheDocument();
    expect(screen.queryByRole("columnheader", { name: "Previsão" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByText("Colunas"));
    await userEvent.click(screen.getByRole("button", { name: "Restaurar" }));
    expect(screen.getByRole("columnheader", { name: "Previsão" })).toBeInTheDocument();
  });

  it("offers the minimum stock on the position, hidden until asked for", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/stock/positions/summary")) return { status: 200, body: { total: 1, in_stock: 1, below_minimum: 0, depleted: 0, value: "120" } };
      if (url.includes("/stock/positions")) return { status: 200, body: { items: [position], next_cursor: null } };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/stock");
    await screen.findByRole("row", { name: /Grão de café/ });
    expect(screen.queryByRole("columnheader", { name: "Mínimo" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByText("Colunas"));
    await userEvent.click(within(screen.getByRole("group", { name: "Colunas da tabela" })).getByRole("checkbox", { name: "Estoque mínimo" }));
    expect(screen.getByRole("columnheader", { name: "Mínimo" })).toBeInTheDocument();
    expect(screen.getByRole("row", { name: /Grão de café/ })).toHaveTextContent("5 KG");
  });

  it("falls back to the usual columns when what is stored is not a list of columns", async () => {
    window.localStorage.setItem("conta-verde.columns.purchases", "{not json");
    mockApi((url) =>
      url === "/api/auth/me"
        ? { status: 200, body: profile }
        : url.includes("/purchase-orders") && !url.includes("status-counts")
          ? { status: 200, body: { items: [order], next_cursor: null } }
          : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/purchases");
    expect(await screen.findByRole("columnheader", { name: "Previsão" })).toBeInTheDocument();
  });
});
