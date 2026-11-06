import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Party, Product, SalesReport } from "./api";
import { mockApi, profile, renderAt, type Reply } from "./testSupport";

const party = (id: string, name: string): Party => ({
  id,
  company_id: "c1",
  name,
  document: null,
  email: null,
  is_customer: true,
  is_supplier: false,
  is_carrier: false,
  is_seller: false,
  address: null,
  contacts: [],
  tags: [],
  document_check: { status: "not_applicable", legal_name: null, situation: null, needs_attention: false },
});
const coffee: Product = {
  id: "pr1",
  company_id: "c1",
  kind: "product",
  name: "Café 500g",
  sku: null,
  unit: "UN",
  ncm: null,
  sale_price: "10",
  cost_price: null,
  minimum_stock: null,
  active: true,
};

const byCustomer: SalesReport = {
  from: "2026-11-01",
  to: "2026-11-30",
  by: "customer",
  sales: 3,
  total: "1000",
  rows: [
    { key: "p1", sales: 2, net: "850", share: "85", cumulative: "85", class: "A" },
    { key: "p2", sales: 1, net: "150", share: "15", cumulative: "100", class: "B" },
  ],
};

const answer =
  (reply: (url: string) => Reply | undefined) =>
  (url: string): Reply => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [] } };
    if (url.includes("/cost-centers")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.includes("/parties")) return { status: 200, body: { items: [party("p1", "Mercado Boa Compra"), party("p2", "Clínica Vida")], next_cursor: null } };
    if (url.includes("/products")) return { status: 200, body: { items: [coffee], next_cursor: null } };
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
  await userEvent.click(await screen.findByRole("button", { name: "Vendas" }));
};

describe("sales report", () => {
  it("asks for this month by customer and shows each row's value, share and ABC class", async () => {
    const fetchMock = mockApi(answer((url) => (url.includes("/sales-reports") ? { status: 200, body: byCustomer } : undefined)));
    await open();

    const first = await screen.findByRole("row", { name: /Mercado Boa Compra/ });
    expect(first).toHaveTextContent("R$ 850,00");
    expect(first).toHaveTextContent("85,00%");
    expect(first).toHaveTextContent("A");
    const second = screen.getByRole("row", { name: /Clínica Vida/ });
    expect(second).toHaveTextContent("100,00%");
    expect(second).toHaveTextContent("B");
    expect(screen.getByText("Total vendido", { selector: ".stat-label" }).closest(".stat-tile")).toHaveTextContent("R$ 1.000,00");
    expect(fetchMock.mock.calls.map(([url]) => String(url))).toContain(
      "/api/companies/c1/sales-reports?by=customer&from=2026-11-01&to=2026-11-30",
    );
  });

  it("asks again for products or sellers and names the sales that have no seller", async () => {
    const fetchMock = mockApi(
      answer((url) => {
        if (!url.includes("/sales-reports")) return undefined;
        if (url.includes("by=seller")) {
          return { status: 200, body: { ...byCustomer, by: "seller", rows: [{ key: null, sales: 3, net: "1000", share: "100", cumulative: "100", class: "A" }] } };
        }
        if (url.includes("by=product")) {
          return { status: 200, body: { ...byCustomer, by: "product", rows: [{ key: "pr1", sales: 3, net: "1000", share: "100", cumulative: "100", class: "A" }] } };
        }
        return { status: 200, body: byCustomer };
      }),
    );
    await open();
    await screen.findByRole("row", { name: /Mercado Boa Compra/ });

    await userEvent.selectOptions(screen.getByLabelText("Agrupar por"), "product");
    expect(await screen.findByRole("row", { name: /Café 500g/ })).toHaveTextContent("R$ 1.000,00");
    await userEvent.selectOptions(screen.getByLabelText("Agrupar por"), "seller");
    expect(await screen.findByRole("row", { name: /Sem vendedor/ })).toBeInTheDocument();
    expect(fetchMock.mock.calls.map(([url]) => String(url)).some((url) => url.includes("by=seller"))).toBe(true);
  });

  it("says so when nothing was sold and shows the server's refusal", async () => {
    let refuse = false;
    mockApi(
      answer((url) =>
        url.includes("/sales-reports")
          ? refuse
            ? { status: 400, body: { error: "more than 2000 sales: narrow the period" } }
            : { status: 200, body: { ...byCustomer, sales: 0, total: "0", rows: [] } }
          : undefined,
      ),
    );
    await open();
    expect(await screen.findByText("Nenhuma venda concluída nesse período.")).toBeInTheDocument();

    refuse = true;
    await userEvent.selectOptions(screen.getByLabelText("Agrupar por"), "seller");
    expect(await screen.findByRole("alert")).toHaveTextContent("more than 2000 sales");
    expect(within(screen.getByRole("alert")).queryByText("Nenhuma venda concluída nesse período.")).toBeNull();
  });
});
