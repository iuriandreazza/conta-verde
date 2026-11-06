import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Entry, Party, Product, Sale, SaleReturn } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const party = (id: string, name: string, is_seller: boolean): Party => ({
  id,
  company_id: "c1",
  name,
  document: null,
  email: null,
  is_customer: !is_seller,
  is_supplier: false,
  is_carrier: false,
  is_seller,
  address: null,
  contacts: [],
  tags: [],
  document_check: { status: "not_applicable", legal_name: null, situation: null, needs_attention: false },
});
const customer = party("p1", "Mercado Boa Compra", false);
const vera = party("p2", "Vera Vendedora", true);

const coffee: Product = {
  id: "pr1",
  company_id: "c1",
  kind: "product",
  name: "Café 500g",
  sku: "cafe",
  unit: "UN",
  ncm: null,
  sale_price: "10.1",
  cost_price: null,
  minimum_stock: null,
  active: true,
};
const term = {
  id: "t1",
  company_id: "c1",
  name: "30/60",
  installments: [
    { days: 30, percent: "50" },
    { days: 60, percent: "50" },
  ],
  active: true,
};
const revenue = { id: "cat1", company_id: "c1", parent_id: null, name: "Vendas", dre_group: "gross_revenue", cash_flow_group: "operating", level: 1, active: true };
const refunds = { ...revenue, id: "cat2", name: "Devoluções", dre_group: "operating_expenses" };
const transfers = { ...revenue, id: "cat3", name: "Transferências", dre_group: "transfers", cash_flow_group: "transfer" };

const order: Sale = {
  id: "s1",
  company_id: "c1",
  number: 3,
  customer_id: "p1",
  seller_id: "p2",
  sold_on: "2026-11-06",
  payment_term_id: "t1",
  category_id: "cat1",
  cost_center_id: null,
  notes: null,
  status: "open",
  quote_id: null,
  contract_id: null,
  cycle_on: null,
  lines: [{ product_id: "pr1", description: "Café 500g", quantity: "3", unit_price: "10.1", discount: "0.3", total: "30" }],
  total: "30",
  receivable_id: null,
};
const completed: Sale = { ...order, id: "s2", number: 4, status: "completed" };
const cancelled: Sale = { ...order, id: "s3", number: 5, status: "cancelled" };

const calls = (fetchMock: ReturnType<typeof mockApi>, method: string) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === method);

const answer =
  (items: Sale[], extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined) =>
  (url: string, init?: RequestInit): Reply => {
    const special = extra(url, init);
    if (special) return special;
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.includes("/parties")) return { status: 200, body: { items: [customer, vera], next_cursor: null } };
    if (url.includes("/products")) return { status: 200, body: { items: [coffee], next_cursor: null } };
    if (url.includes("/payment-terms")) return { status: 200, body: { items: [term], next_cursor: null } };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [revenue, refunds, transfers] } };
    if (url.includes("/cost-centers")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.includes("/sales")) return { status: 200, body: { items, next_cursor: null } };
    return { status: 404, body: { error: "not found" } };
  };

const formPanel = async (heading: string) => {
  const panel = within((await screen.findByRole("heading", { name: heading })).closest("aside")!);
  await panel.findByLabelText("Cliente");
  return panel;
};

describe("sales", () => {
  it("lists the orders and sales with their customer, seller, date, total and status", async () => {
    mockApi(answer([order, completed, cancelled]));
    renderAt("/companies/c1/sales");

    const row = (await screen.findAllByRole("row", { name: /Mercado Boa Compra/ }))[0];
    expect(row).toHaveTextContent("3");
    expect(row).toHaveTextContent("Vera Vendedora");
    expect(row).toHaveTextContent("06/11/2026");
    expect(row).toHaveTextContent("R$ 30,00");
    expect(row).toHaveTextContent("Pedido");
    expect(screen.getByRole("row", { name: /Mercado Boa Compra.*Venda concluída/ })).toHaveTextContent("4");
    expect(screen.getByRole("row", { name: /Mercado Boa Compra.*Cancelada/ })).toHaveTextContent("5");
    // An order can be changed and completed; a completed sale has returns; a cancelled one has nothing.
    expect(screen.getByRole("button", { name: "Alterar o pedido 3" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Alterar o pedido 4" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Devoluções da venda 4" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancelar a venda 5" })).not.toBeInTheDocument();
  });

  it("writes an order: only sellers are offered, only revenue categories in use, and the lines go as the quote's do", async () => {
    const fetchMock = mockApi(
      answer([], (url, init) =>
        url.endsWith("/sales") && init?.method === "POST" ? { status: 201, body: { ...order, id: "s9" } } : undefined,
      ),
    );
    renderAt("/companies/c1/sales");
    const form = await formPanel("Novo pedido");

    expect(within(form.getByLabelText("Vendedor")).getAllByRole("option").map((o) => o.textContent)).toEqual(["Nenhum", "Vera Vendedora"]);
    expect(within(form.getByLabelText("Categoria da receita")).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Escolha uma categoria",
      "Vendas",
      "Devoluções",
    ]);
    await userEvent.selectOptions(form.getByLabelText("Cliente"), "p1");
    await userEvent.selectOptions(form.getByLabelText("Vendedor"), "p2");
    await userEvent.selectOptions(form.getByLabelText("Prazo de pagamento"), "t1");
    await userEvent.selectOptions(form.getByLabelText("Categoria da receita"), "cat1");
    await userEvent.selectOptions(form.getByLabelText("Produto ou serviço do item 1"), "pr1");
    await userEvent.clear(form.getByLabelText("Quantidade do item 1"));
    await userEvent.type(form.getByLabelText("Quantidade do item 1"), "3");
    fireEvent.change(form.getByLabelText("Data da venda"), { target: { value: "2026-11-06" } });
    await userEvent.click(form.getByRole("button", { name: "Criar pedido" }));

    expect(await screen.findByText(/Pedido nº 3 criado/)).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({
      customer_id: "p1",
      seller_id: "p2",
      sold_on: "2026-11-06",
      payment_term_id: "t1",
      category_id: "cat1",
      lines: [{ product_id: "pr1", description: "Café 500g", quantity: "3", unit_price: "10.1" }],
    });
  });

  it("shows the installments the term will make before an order is completed, then completes it", async () => {
    const fetchMock = mockApi(
      answer([order], (url, init) =>
        url.endsWith("/sales/s1/complete") && init?.method === "POST"
          ? { status: 200, body: { ...order, status: "completed", receivable_id: "e1" } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/sales");
    await userEvent.click(await screen.findByRole("button", { name: "Concluir a venda 3" }));
    const region = within(screen.getByRole("region", { name: "Conclusão da venda 3" }));

    // 30 days and 60 days after 6 November, half and half of 30.
    expect(region.getByText(/06\/12\/2026 · R\$ 15,00/)).toBeInTheDocument();
    expect(region.getByText(/05\/01\/2027 · R\$ 15,00/)).toBeInTheDocument();
    expect(region.getByText(/pelo prazo 30\/60/)).toBeInTheDocument();
    await userEvent.click(region.getByRole("button", { name: "Concluir venda" }));

    expect(await screen.findByText(/Venda nº 3 concluída: a conta a receber está nos lançamentos/)).toBeInTheDocument();
    expect(calls(fetchMock, "POST")).toHaveLength(1);
    expect(await screen.findByRole("row", { name: /Mercado Boa Compra.*Venda concluída/ })).toBeInTheDocument();
  });

  it("cancels a completed sale, saying that its receivable goes with it, and shows why when it is refused", async () => {
    let refuse = true;
    mockApi(
      answer([completed], (url, init) => {
        if (!url.endsWith("/sales/s2/cancel") || init?.method !== "POST") return undefined;
        if (refuse) {
          refuse = false;
          return { status: 409, body: { error: "the receivable of this sale has settlements: reverse them before cancelling it" } };
        }
        return { status: 200, body: { ...completed, status: "cancelled" } };
      }),
    );
    renderAt("/companies/c1/sales");
    await userEvent.click(await screen.findByRole("button", { name: "Cancelar a venda 4" }));
    // Nothing happens until it is confirmed, and the confirmation says what it costs.
    const region = within(screen.getByRole("region", { name: "Cancelamento da venda 4" }));
    expect(region.getByText(/cancela também a conta a receber que ela gerou/)).toBeInTheDocument();
    await userEvent.click(region.getByRole("button", { name: "Cancelar a venda" }));
    expect(await region.findByRole("alert")).toHaveTextContent("reverse them before cancelling it");
    await userEvent.click(region.getByRole("button", { name: "Cancelar a venda" }));
    expect(await screen.findByText("Venda nº 4 cancelada e a conta a receber também.")).toBeInTheDocument();
    expect(await screen.findByRole("row", { name: /Mercado Boa Compra.*Cancelada/ })).toBeInTheDocument();
  });

  it("lists what was returned and records another return, in decimal text", async () => {
    const earlier: SaleReturn = {
      id: "r1",
      sale_id: "s2",
      returned_on: "2026-11-10",
      reason: "Estragou",
      items: [{ line: 1, quantity: "1" }],
      refund: "10.03",
      refund_entry_id: "e9",
    };
    const fetchMock = mockApi(
      answer([completed], (url, init) => {
        if (url.endsWith("/sales/s2/returns") && init?.method === "POST") {
          return { status: 201, body: { ...earlier, id: "r2", refund: "20.04" } };
        }
        if (url.endsWith("/sales/s2/returns")) return { status: 200, body: { items: [earlier] } };
        return undefined;
      }),
    );
    renderAt("/companies/c1/sales");
    await userEvent.click(await screen.findByRole("button", { name: "Devoluções da venda 4" }));
    const region = within(screen.getByRole("region", { name: "Devoluções da venda 4" }));

    expect(await region.findByText(/reembolso de R\$ 10,03 · Café 500g × 1 · Estragou/)).toBeInTheDocument();
    // Nothing typed: nothing to record.
    await userEvent.selectOptions(region.getByLabelText("Categoria do reembolso"), "cat2");
    await userEvent.click(region.getByRole("button", { name: "Registrar devolução" }));
    expect(await region.findByRole("alert")).toHaveTextContent("Informe a quantidade devolvida");
    await userEvent.type(region.getByLabelText("Quantidade devolvida de Café 500g"), "2");
    fireEvent.change(region.getByLabelText("Data da devolução"), { target: { value: "2026-11-11" } });
    fireEvent.change(region.getByLabelText("Reembolso vence em"), { target: { value: "2026-11-20" } });
    await userEvent.click(region.getByRole("button", { name: "Registrar devolução" }));

    await vi.waitFor(() => expect(calls(fetchMock, "POST")).toHaveLength(1));
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({
      returned_on: "2026-11-11",
      items: [{ line: 1, quantity: "2" }],
      refund_category_id: "cat2",
      refund_due_on: "2026-11-20",
    });
  });

  it("shows the list and nothing to write to a role that only reads sales", async () => {
    mockApi(
      answer([order], (url) =>
        url === "/api/auth/me"
          ? {
              status: 200,
              body: {
                ...profile,
                companies: [{ id: "c1", name: "Padaria", role: "viewer", permissions: { ...OWNER_PERMISSIONS, sales: "read" } }],
              },
            }
          : undefined,
      ),
    );
    renderAt("/companies/c1/sales");
    await screen.findByRole("row", { name: /Mercado Boa Compra/ });
    expect(screen.queryByRole("heading", { name: "Novo pedido" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Concluir a venda/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Cancelar a venda/ })).not.toBeInTheDocument();
  });
});

describe("entries made by sales", () => {
  const entry = (patch: Partial<Entry>): Entry => ({
    id: "e1",
    company_id: "c1",
    kind: "receivable",
    description: "Venda nº 4",
    party_id: "p1",
    category_id: "cat1",
    cost_center_id: null,
    competence_date: "2026-11-06",
    installments: [{ number: 1, due_date: "2026-12-06", amount: "30", settled: "0" }],
    total: "30",
    recurrence_id: null,
    sale_id: "s2",
    sale_return_id: null,
    voided: false,
    ...patch,
  });

  it("says where a receivable came from, and a cancelled one is marked and cannot be edited or settled", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/categories")) return { status: 200, body: { items: [revenue] } };
      if (url.includes("/entries")) {
        return {
          status: 200,
          body: {
            items: [entry({}), entry({ id: "e2", description: "Venda nº 5", voided: true, sale_id: "s3" })],
            next_cursor: null,
          },
        };
      }
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/entries");

    const live = await screen.findByRole("row", { name: /Venda nº 4/ });
    expect(live).toHaveTextContent("De uma venda");
    // What a sale made follows the sale: it is not edited as an entry of its own.
    expect(within(live).queryByRole("button", { name: "Alterar Venda nº 4" })).not.toBeInTheDocument();
    expect(within(live).getByRole("button", { name: "Baixas de Venda nº 4" })).toBeInTheDocument();
    const gone = screen.getByRole("row", { name: /Venda nº 5/ });
    expect(gone).toHaveTextContent("Cancelado");
    expect(within(gone).queryByRole("button", { name: /Alterar/ })).not.toBeInTheDocument();
    expect(within(gone).queryByRole("button", { name: /Baixas/ })).not.toBeInTheDocument();
    expect(within(gone).getByRole("button", { name: "Histórico de Venda nº 5" })).toBeInTheDocument();
  });
});
