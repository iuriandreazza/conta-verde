import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Party, Product, PurchaseOrder, PurchaseReceipt } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const party = (id: string, name: string, is_supplier: boolean): Party => ({
  id,
  company_id: "c1",
  name,
  document: null,
  email: null,
  is_customer: !is_supplier,
  is_supplier,
  is_carrier: false,
  is_seller: false,
  address: null,
  contacts: [],
  tags: [],
  document_check: { status: "not_applicable", legal_name: null, situation: null, needs_attention: false },
});
const supplier = party("p1", "Torrefação Serra", true);
const customer = party("p2", "Mercado Boa Compra", false);

const beans: Product = {
  id: "pr1",
  company_id: "c1",
  kind: "product",
  name: "Grão de café 1kg",
  sku: "grao",
  unit: "KG",
  ncm: null,
  sale_price: "40",
  cost_price: "25.5",
  minimum_stock: null,
  active: true,
};
const term = {
  id: "t1",
  company_id: "c1",
  name: "30 dias",
  installments: [{ days: 30, percent: "100" }],
  active: true,
};
const expense = { id: "cat1", company_id: "c1", parent_id: null, name: "Matéria-prima", dre_group: "cost_of_goods", cash_flow_group: "operating", level: 1, active: true };
const transfers = { ...expense, id: "cat3", name: "Transferências", dre_group: "transfers", cash_flow_group: "transfer" };

const draft: PurchaseOrder = {
  id: "o1",
  company_id: "c1",
  number: 1,
  supplier_id: "p1",
  ordered_on: "2026-11-06",
  expected_on: "2026-11-20",
  payment_term_id: "t1",
  category_id: "cat1",
  cost_center_id: null,
  notes: null,
  status: "draft",
  invoice_key: null,
  lines: [{ product_id: "pr1", description: "Grão de café 1kg", quantity: "10", unit_price: "25.5", discount: "0", total: "255" }],
  total: "255",
};
const approved: PurchaseOrder = { ...draft, id: "o2", number: 2, status: "approved" };
const partial: PurchaseOrder = { ...draft, id: "o3", number: 3, status: "partially_received" };
const received: PurchaseOrder = { ...draft, id: "o4", number: 4, status: "received" };
const cancelled: PurchaseOrder = { ...draft, id: "o5", number: 5, status: "cancelled" };

const receipt = (patch: Partial<PurchaseReceipt>): PurchaseReceipt => ({
  id: "r1",
  purchase_id: "o3",
  received_on: "2026-11-10",
  notes: null,
  items: [{ line: 1, quantity: "4", value: "102" }],
  total: "102",
  payable_id: "e1",
  ...patch,
});

const calls = (fetchMock: ReturnType<typeof mockApi>, method: string) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === method);

const answer =
  (items: PurchaseOrder[], extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined) =>
  (url: string, init?: RequestInit): Reply => {
    const special = extra(url, init);
    if (special) return special;
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.includes("/parties")) return { status: 200, body: { items: [supplier, customer], next_cursor: null } };
    if (url.includes("/products")) return { status: 200, body: { items: [beans], next_cursor: null } };
    if (url.includes("/payment-terms")) return { status: 200, body: { items: [term], next_cursor: null } };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [expense, transfers] } };
    if (url.includes("/cost-centers")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.includes("/purchase-orders/status-counts")) {
      const of = (status: PurchaseOrder["status"]) => items.filter((order) => order.status === status).length;
      return {
        status: 200,
        body: { draft: of("draft"), approved: of("approved"), partially_received: of("partially_received"), received: of("received"), cancelled: of("cancelled") },
      };
    }
    if (url.includes("/purchase-orders")) return { status: 200, body: { items, next_cursor: null } };
    return { status: 404, body: { error: "not found" } };
  };

const readOnly = (url: string): Reply | undefined =>
  url === "/api/auth/me"
    ? {
        status: 200,
        body: {
          ...profile,
          companies: [{ id: "c1", name: "Padaria", role: "viewer", permissions: { ...OWNER_PERMISSIONS, purchases: "read" } }],
        },
      }
    : undefined;

describe("purchases", () => {
  it("lists the orders with their supplier, date, total and status in words", async () => {
    mockApi(answer([draft, approved, partial, received, cancelled]));
    renderAt("/companies/c1/purchases");

    const row = (await screen.findAllByRole("row", { name: /Torrefação Serra/ }))[0];
    expect(row).toHaveTextContent("1");
    expect(row).toHaveTextContent("06/11/2026");
    expect(row).toHaveTextContent("R$ 255,00");
    expect(row).toHaveTextContent("Previsto para 20/11/2026");
    expect(row).toHaveTextContent("Rascunho");
    expect(screen.getByRole("row", { name: /Torrefação Serra.*Aprovado/ })).toHaveTextContent("2");
    expect(screen.getByRole("row", { name: /Torrefação Serra.*Recebido em parte/ })).toHaveTextContent("3");
    expect(screen.getByRole("row", { name: /Torrefação Serra.*Recebido(?! em parte)/ })).toHaveTextContent("4");
    expect(screen.getByRole("row", { name: /Torrefação Serra.*Cancelado/ })).toHaveTextContent("5");
  });

  it("counts the orders by status in tiles that filter the list", async () => {
    const fetchMock = mockApi(answer([draft, approved, partial, received, cancelled, { ...draft, id: "o6", number: 6 }]));
    renderAt("/companies/c1/purchases");

    const tiles = within(await screen.findByRole("group", { name: "Situação" }));
    await vi.waitFor(() => expect(tiles.getByRole("button", { name: /Todas/ })).toHaveTextContent("6"));
    expect(tiles.getByRole("button", { name: /Rascunho/ })).toHaveTextContent("2");
    expect(tiles.getByRole("button", { name: /Aprovado/ })).toHaveTextContent("1");
    expect(tiles.getByRole("button", { name: /Cancelado/ })).toHaveTextContent("1");

    await userEvent.click(tiles.getByRole("button", { name: /Aprovado/ }));
    expect(tiles.getByRole("button", { name: /Aprovado/ })).toHaveAttribute("aria-pressed", "true");
    await vi.waitFor(() => expect(fetchMock.mock.calls.some(([url]) => String(url).includes("/purchase-orders?status=approved"))).toBe(true));
    // The counts follow the search, never the status: every tile keeps its number.
    expect(fetchMock.mock.calls.some(([url]) => String(url).includes("status-counts?status"))).toBe(false);
  });

  it("offers each status only the actions it allows: a draft is edited and approved, an approved one received", async () => {
    mockApi(answer([draft, approved, partial, received, cancelled]));
    renderAt("/companies/c1/purchases");
    await screen.findAllByRole("row", { name: /Torrefação Serra/ });

    expect(screen.getByRole("button", { name: "Alterar a compra 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Aprovar a compra 1" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Receber a compra 1" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Alterar a compra 2" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Aprovar a compra 2" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Receber a compra 2" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancelar a compra 2" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Encerrar a compra 2" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Receber a compra 3" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Encerrar a compra 3" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancelar a compra 3" })).not.toBeInTheDocument();
    for (const number of [4, 5]) {
      expect(screen.queryByRole("button", { name: new RegExp(`(Receber|Encerrar|Cancelar|Alterar|Aprovar) a compra ${number}`) })).not.toBeInTheDocument();
    }
  });

  it("writes an order: only suppliers are offered, the cost fills from the product, and the body is the order's", async () => {
    const fetchMock = mockApi(
      answer([], (url, init) =>
        url.endsWith("/purchase-orders") && init?.method === "POST" ? { status: 201, body: draft } : undefined,
      ),
    );
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Novo" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Nova compra" }));
    const form = within((await screen.findByRole("heading", { name: "Nova compra" })).closest("aside")!);
    await form.findByLabelText("Fornecedor");

    expect(within(form.getByLabelText("Fornecedor")).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Escolha um fornecedor",
      "Torrefação Serra",
    ]);
    expect(within(form.getByLabelText("Categoria da despesa")).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Escolha uma categoria",
      "Matéria-prima",
    ]);
    await userEvent.selectOptions(form.getByLabelText("Fornecedor"), "p1");
    await userEvent.selectOptions(form.getByLabelText("Prazo de pagamento"), "t1");
    await userEvent.selectOptions(form.getByLabelText("Categoria da despesa"), "cat1");
    await userEvent.selectOptions(form.getByLabelText("Produto ou serviço do item 1"), "pr1");
    // The cost of the product, not its sale price, and no price lists to choose from.
    expect(form.getByLabelText("Custo do item 1")).toHaveValue("25,5");
    expect(form.queryByLabelText("Tabela de preços")).not.toBeInTheDocument();
    await userEvent.clear(form.getByLabelText("Quantidade do item 1"));
    await userEvent.type(form.getByLabelText("Quantidade do item 1"), "10");
    fireEvent.change(form.getByLabelText("Data do pedido"), { target: { value: "2026-11-06" } });
    fireEvent.change(form.getByLabelText("Previsão de entrega"), { target: { value: "2026-11-20" } });
    await userEvent.click(form.getByRole("button", { name: "Criar pedido" }));

    expect(await screen.findByText("Compra nº 1 criada.")).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({
      supplier_id: "p1",
      ordered_on: "2026-11-06",
      expected_on: "2026-11-20",
      payment_term_id: "t1",
      category_id: "cat1",
      lines: [{ product_id: "pr1", description: "Grão de café 1kg", quantity: "10", unit_price: "25.5" }],
    });
  });

  it("loads a draft into the form to change it, and sends the whole order back", async () => {
    const fetchMock = mockApi(
      answer([draft], (url, init) =>
        url.endsWith("/purchase-orders/o1") && init?.method === "PUT" ? { status: 200, body: { ...draft, notes: "Urgente" } } : undefined,
      ),
    );
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Alterar a compra 1" }));
    const form = within(screen.getByRole("heading", { name: "Alterar a compra 1" }).closest("aside")!);
    expect(form.getByLabelText("Fornecedor")).toHaveValue("p1");
    expect(form.getByLabelText("Quantidade do item 1")).toHaveValue("10");
    await userEvent.type(form.getByLabelText("Observações"), "Urgente");
    await userEvent.click(form.getByRole("button", { name: "Salvar alterações" }));

    expect(await screen.findByText("Compra nº 1 atualizada.")).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "PUT")[0][1]?.body))).toMatchObject({
      supplier_id: "p1",
      notes: "Urgente",
      expected_on: "2026-11-20",
      lines: [{ product_id: "pr1", quantity: "10", unit_price: "25.5" }],
    });
  });

  it("approves a draft only after it is confirmed", async () => {
    const fetchMock = mockApi(
      answer([draft], (url, init) =>
        url.endsWith("/purchase-orders/o1/approve") && init?.method === "POST" ? { status: 200, body: { ...draft, status: "approved" } } : undefined,
      ),
    );
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Aprovar a compra 1" }));
    const region = within(screen.getByRole("region", { name: "Confirmação da compra 1" }));
    expect(region.getByText(/os itens não mudam/)).toBeInTheDocument();
    expect(calls(fetchMock, "POST")).toHaveLength(0);
    await userEvent.click(region.getByRole("button", { name: "Aprovar compra" }));

    expect(await screen.findByText("Compra nº 1 aprovada.")).toBeInTheDocument();
    expect(calls(fetchMock, "POST")).toHaveLength(1);
    expect(await screen.findByRole("row", { name: /Torrefação Serra.*Aprovado/ })).toBeInTheDocument();
  });

  it("receives what is left of the lines and sends only the ones with a quantity", async () => {
    const two: PurchaseOrder = {
      ...partial,
      lines: [
        ...partial.lines,
        { product_id: "pr1", description: "Filtro de papel", quantity: "5", unit_price: "2", discount: "0", total: "10" },
      ],
      total: "265",
    };
    const fetchMock = mockApi(
      answer([two], (url, init) => {
        if (url.endsWith("/purchase-orders/o3/receipts") && init?.method === "POST") {
          return { status: 201, body: receipt({ id: "r2", items: [{ line: 1, quantity: "6", value: "153" }], total: "153" }) };
        }
        if (url.endsWith("/purchase-orders/o3/receipts")) return { status: 200, body: { items: [receipt({})] } };
        if (url.endsWith("/purchase-orders/o3")) return { status: 200, body: { ...two, status: "received" } };
        return undefined;
      }),
    );
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Receber a compra 3" }));
    const region = within(screen.getByRole("region", { name: "Recebimento da compra 3" }));

    // 4 of 10 arrived before: 6 are left on the first line and all 5 of the second.
    const first = await region.findByLabelText("Quantidade recebida de Grão de café 1kg");
    expect(first).toHaveValue("6");
    expect(region.getByLabelText("Quantidade recebida de Filtro de papel")).toHaveValue("5");
    await userEvent.clear(region.getByLabelText("Quantidade recebida de Filtro de papel"));
    fireEvent.change(region.getByLabelText("Data do recebimento"), { target: { value: "2026-11-12" } });
    await userEvent.click(region.getByRole("button", { name: "Registrar recebimento" }));

    expect(await screen.findByText(/Recebimento registrado: a conta a pagar de R\$ 153,00 está nos lançamentos/)).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({
      received_on: "2026-11-12",
      items: [{ line: 1, quantity: "6" }],
    });
    expect(await screen.findByRole("row", { name: /Torrefação Serra.*Recebido(?! em parte)/ })).toBeInTheDocument();
  });

  it("refuses a receipt with no quantity before asking the API", async () => {
    const fetchMock = mockApi(
      answer([approved], (url) =>
        url.endsWith("/purchase-orders/o2/receipts") ? { status: 200, body: { items: [] } } : undefined,
      ),
    );
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Receber a compra 2" }));
    const region = within(screen.getByRole("region", { name: "Recebimento da compra 2" }));
    await userEvent.clear(await region.findByLabelText("Quantidade recebida de Grão de café 1kg"));
    await userEvent.click(region.getByRole("button", { name: "Registrar recebimento" }));

    expect(await region.findByRole("alert")).toHaveTextContent("Informe a quantidade recebida de pelo menos um item");
    expect(calls(fetchMock, "POST")).toHaveLength(0);
  });

  it("shows what each line received and what is left, and finishes a receipt whose payable failed", async () => {
    const fetchMock = mockApi(
      answer([partial], (url, init) => {
        if (url.endsWith("/receipts/r1/finish") && init?.method === "POST") return { status: 200, body: receipt({}) };
        if (url.endsWith("/purchase-orders/o3/receipts")) return { status: 200, body: { items: [receipt({ payable_id: null })] } };
        return undefined;
      }),
    );
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Detalhes da compra 3" }));
    const region = within(screen.getByRole("region", { name: "Detalhes da compra 3" }));

    const line = await region.findByRole("row", { name: /Grão de café 1kg/ });
    // Ordered, received, left, unit cost and value.
    await vi.waitFor(() =>
      expect(within(line).getAllByRole("cell").map((cell) => cell.textContent)).toEqual(["Grão de café 1kg", "10", "4", "6", "R$ 25,50", "R$ 255,00"]),
    );
    expect(region.getByText(/10\/11\/2026 · Grão de café 1kg × 4 · R\$ 102,00/)).toBeInTheDocument();
    expect(region.queryByRole("link", { name: /Conta a pagar/ })).not.toBeInTheDocument();
    await userEvent.click(region.getByRole("button", { name: "Concluir o recebimento de 10/11/2026" }));

    expect(await region.findByRole("link", { name: "Conta a pagar nos lançamentos" })).toHaveAttribute("href", "/companies/c1/entries");
    expect(calls(fetchMock, "POST")).toHaveLength(1);
  });

  it("links the payable of a receipt to the entries", async () => {
    mockApi(
      answer([partial], (url) =>
        url.endsWith("/purchase-orders/o3/receipts") ? { status: 200, body: { items: [receipt({})] } } : undefined,
      ),
    );
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Detalhes da compra 3" }));
    const region = within(screen.getByRole("region", { name: "Detalhes da compra 3" }));
    expect(await region.findByRole("link", { name: "Conta a pagar nos lançamentos" })).toHaveAttribute("href", "/companies/c1/entries");
    expect(region.queryByRole("button", { name: "Concluir o recebimento de 10/11/2026" })).not.toBeInTheDocument();
  });

  it("closes a partly received order and tells what it gives up", async () => {
    mockApi(
      answer([partial], (url, init) =>
        url.endsWith("/purchase-orders/o3/close") && init?.method === "POST" ? { status: 200, body: { ...partial, status: "received" } } : undefined,
      ),
    );
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Encerrar a compra 3" }));
    const region = within(screen.getByRole("region", { name: "Confirmação da compra 3" }));
    expect(region.getByText(/o que ainda não chegou é dispensado/)).toBeInTheDocument();
    await userEvent.click(region.getByRole("button", { name: "Encerrar a compra" }));
    expect(await screen.findByText("Compra nº 3 encerrada: o que não chegou foi dispensado.")).toBeInTheDocument();
  });

  it("shows the list and nothing to write to a role that only reads purchases", async () => {
    mockApi(answer([draft, approved, partial], readOnly));
    renderAt("/companies/c1/purchases");
    await screen.findAllByRole("row", { name: /Torrefação Serra/ });

    expect(screen.queryByRole("button", { name: "Nova compra" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /(Alterar|Aprovar|Receber|Encerrar|Cancelar) a compra/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Detalhes da compra 1" })).toBeInTheDocument();
  });

  it("offers no finishing of a receipt to a role that only reads", async () => {
    mockApi(
      answer([partial], (url) => {
        if (url.endsWith("/purchase-orders/o3/receipts")) return { status: 200, body: { items: [receipt({ payable_id: null })] } };
        return readOnly(url);
      }),
    );
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Detalhes da compra 3" }));
    const region = within(screen.getByRole("region", { name: "Detalhes da compra 3" }));
    expect(await region.findByText(/conta a pagar ainda não lançada/)).toBeInTheDocument();
    expect(region.queryByRole("button", { name: "Concluir o recebimento de 10/11/2026" })).not.toBeInTheDocument();
  });
});
