import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Party, Product, PublicQuote, Quote } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const customer: Party = {
  id: "p1",
  company_id: "c1",
  name: "Mercado Boa Compra",
  document: null,
  email: "compras@boacompra.example",
  is_customer: true,
  is_supplier: false,
  is_carrier: false,
  is_seller: false,
  address: null,
  contacts: [],
  tags: [],
  document_check: { status: "not_applicable", legal_name: null, situation: null, needs_attention: false },
};

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
const bread: Product = { ...coffee, id: "pr2", name: "Pão francês", sku: "pao", sale_price: "3.33" };
const retired: Product = { ...coffee, id: "pr3", name: "Descontinuado", active: false };
const term = { id: "t1", company_id: "c1", name: "30/60", installments: [], active: true };
const revenue = { id: "cat1", company_id: "c1", parent_id: null, name: "Vendas", dre_group: "gross_revenue", cash_flow_group: "operating", level: 1, active: true };

const quote: Quote = {
  id: "q1",
  company_id: "c1",
  number: 7,
  customer_id: "p1",
  issued_on: "2026-11-06",
  valid_until: "2026-11-20",
  payment_term_id: "t1",
  notes: "Entrega em 5 dias",
  status: "draft",
  lines: [
    { product_id: "pr1", description: "Café 500g", quantity: "3", unit_price: "10.1", discount: "0.3", total: "30" },
    { product_id: "pr2", description: "Pão francês", quantity: "1.5", unit_price: "3.33", discount: "0", total: "5" },
  ],
  total: "35",
};
const sentQuote: Quote = { ...quote, id: "q2", number: 8, status: "sent" };
const approvedQuote: Quote = { ...quote, id: "q3", number: 9, status: "approved" };

const sentCalls = (fetchMock: ReturnType<typeof mockApi>, method: string) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === method);

const answer =
  (items: Quote[], extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined) =>
  (url: string, init?: RequestInit): Reply => {
    const special = extra(url, init);
    if (special) return special;
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.includes("/parties")) return { status: 200, body: { items: [customer], next_cursor: null } };
    if (url.includes("/products")) return { status: 200, body: { items: [coffee, bread, retired], next_cursor: null } };
    if (url.includes("/payment-terms")) return { status: 200, body: { items: [term], next_cursor: null } };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [revenue] } };
    if (url.includes("/cost-centers")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.includes("/quotes")) return { status: 200, body: { items, next_cursor: null } };
    return { status: 404, body: { error: "not found" } };
  };

const formPanel = async (heading: string) => {
  const panel = within((await screen.findByRole("heading", { name: heading })).closest("aside")!);
  await panel.findByLabelText("Cliente");
  return panel;
};

describe("quotes", () => {
  it("lists the quotes with their number, customer, validity, total and status", async () => {
    mockApi(answer([quote, sentQuote, approvedQuote]));
    renderAt("/companies/c1/quotes");

    const row = (await screen.findAllByRole("row", { name: /Mercado Boa Compra/ }))[0];
    expect(row).toHaveTextContent("7");
    expect(row).toHaveTextContent("2 itens");
    expect(row).toHaveTextContent("20/11/2026");
    expect(row).toHaveTextContent("R$ 35,00");
    expect(row).toHaveTextContent("Rascunho");
    expect(screen.getAllByText("Enviado").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Aprovado").length).toBeGreaterThan(0);
    expect(screen.getByRole("link", { name: "PDF do orçamento 7" })).toHaveAttribute(
      "href",
      "/api/companies/c1/quotes/q1/pdf",
    );
  });

  it("writes a quote: the product fills the line, the totals are worked out as the API will and decimals go as text", async () => {
    const fetchMock = mockApi(
      answer([], (url, init) =>
        url.endsWith("/quotes") && init?.method === "POST" ? { status: 201, body: { ...quote, id: "q9" } } : undefined,
      ),
    );
    renderAt("/companies/c1/quotes");
    const form = await formPanel("Novo orçamento");

    // Only products in use are offered.
    expect(
      within(form.getByLabelText("Produto ou serviço do item 1")).getAllByRole("option").map((option) => option.textContent),
    ).toEqual(["Escolha", "Café 500g", "Pão francês"]);
    await userEvent.selectOptions(form.getByLabelText("Cliente"), "p1");
    await userEvent.selectOptions(form.getByLabelText("Produto ou serviço do item 1"), "pr1");
    expect(form.getByLabelText("Descrição do item 1")).toHaveValue("Café 500g");
    expect(form.getByLabelText("Preço do item 1")).toHaveValue("10,1");
    await userEvent.clear(form.getByLabelText("Quantidade do item 1"));
    await userEvent.type(form.getByLabelText("Quantidade do item 1"), "3");
    await userEvent.type(form.getByLabelText("Desconto do item 1"), "0,30");
    await userEvent.click(form.getByRole("button", { name: "Adicionar item" }));
    await userEvent.selectOptions(form.getByLabelText("Produto ou serviço do item 2"), "pr2");
    await userEvent.clear(form.getByLabelText("Quantidade do item 2"));
    await userEvent.type(form.getByLabelText("Quantidade do item 2"), "1,5");
    // 3 x 10,10 less 0,30 and 1,5 x 3,33 = 4,995 rounded up to 5.
    expect(form.getByText("Total: R$ 35,00")).toBeInTheDocument();
    await userEvent.selectOptions(form.getByLabelText("Prazo de pagamento"), "t1");
    fireEvent.change(form.getByLabelText("Emitido em"), { target: { value: "2026-11-06" } });
    fireEvent.change(form.getByLabelText("Válido até"), { target: { value: "2026-11-20" } });
    await userEvent.type(form.getByLabelText("Observações"), "Entrega em 5 dias");
    await userEvent.click(form.getByRole("button", { name: "Criar orçamento" }));

    expect(await screen.findByText("Orçamento nº 7 criado.")).toBeInTheDocument();
    expect(JSON.parse(String(sentCalls(fetchMock, "POST")[0][1]?.body))).toEqual({
      customer_id: "p1",
      issued_on: "2026-11-06",
      valid_until: "2026-11-20",
      payment_term_id: "t1",
      notes: "Entrega em 5 dias",
      lines: [
        { product_id: "pr1", description: "Café 500g", quantity: "3", unit_price: "10.1", discount: "0.30" },
        { product_id: "pr2", description: "Pão francês", quantity: "1.5", unit_price: "3.33" },
      ],
    });
  });

  it("does not send a quote whose line is not worth what it says, and says which", async () => {
    const fetchMock = mockApi(answer([]));
    renderAt("/companies/c1/quotes");
    const form = await formPanel("Novo orçamento");

    await userEvent.selectOptions(form.getByLabelText("Cliente"), "p1");
    await userEvent.click(form.getByRole("button", { name: "Criar orçamento" }));
    expect(await screen.findByText("Escolha o produto ou serviço do item 1.")).toBeInTheDocument();

    await userEvent.selectOptions(form.getByLabelText("Produto ou serviço do item 1"), "pr1");
    await userEvent.type(form.getByLabelText("Desconto do item 1"), "10,11");
    expect(form.getByText("Há itens com valores que não são números válidos.")).toBeInTheDocument();
    await userEvent.click(form.getByRole("button", { name: "Criar orçamento" }));
    expect(await screen.findByText(/O item 1 precisa de quantidade/)).toBeInTheDocument();
    expect(sentCalls(fetchMock, "POST")).toHaveLength(0);
  });

  it("changes a sent quote, warning that it goes back to a draft, and shows what it points at even if retired", async () => {
    const withRetired: Quote = {
      ...sentQuote,
      lines: [{ ...sentQuote.lines[0], product_id: "pr3", description: "Descontinuado" }],
    };
    const fetchMock = mockApi(
      answer([withRetired], (url, init) =>
        url.endsWith("/quotes/q2") && init?.method === "PUT"
          ? { status: 200, body: { ...withRetired, status: "draft", valid_until: "2026-12-01" } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/quotes");
    await userEvent.click(await screen.findByRole("button", { name: "Alterar o orçamento 8" }));
    const form = await formPanel("Alterar o orçamento 8");

    expect(form.getByText(/devolve a rascunho/)).toBeInTheDocument();
    expect(form.getByLabelText("Produto ou serviço do item 1")).toHaveValue("pr3");
    expect(form.getByLabelText("Quantidade do item 1")).toHaveValue("3");
    fireEvent.change(form.getByLabelText("Válido até"), { target: { value: "2026-12-01" } });
    await userEvent.click(form.getByRole("button", { name: "Salvar alterações" }));

    expect(await screen.findByText(/Voltou a ser rascunho e o link enviado deixou de valer/)).toBeInTheDocument();
    expect(JSON.parse(String(sentCalls(fetchMock, "PUT")[0][1]?.body)).valid_until).toBe("2026-12-01");
    const row = await screen.findByRole("row", { name: /Mercado Boa Compra/ });
    expect(row).toHaveTextContent("Rascunho");
  });

  it("sends the link to the customer's e-mail and shows it once, saying when the e-mail did not go out", async () => {
    const fetchMock = mockApi(
      answer([quote], (url, init) =>
        url.endsWith("/quotes/q1/send") && init?.method === "POST"
          ? {
              status: 200,
              body: { quote: { ...quote, status: "sent" }, link: "https://erp.example/quote/abc", emailed: false },
            }
          : undefined,
      ),
    );
    renderAt("/companies/c1/quotes");
    await userEvent.click(await screen.findByRole("button", { name: "Enviar o orçamento 7" }));
    const region = within(screen.getByRole("region", { name: "Envio do orçamento 7" }));

    expect(region.getByLabelText("E-mail do cliente")).toHaveValue("compras@boacompra.example");
    await userEvent.click(region.getByRole("button", { name: "Enviar link" }));

    expect(await region.findByText(/O e-mail não saiu/)).toBeInTheDocument();
    expect(region.getByLabelText("Link para o cliente")).toHaveValue("https://erp.example/quote/abc");
    expect(JSON.parse(String(sentCalls(fetchMock, "POST")[0][1]?.body))).toEqual({ to: "compras@boacompra.example" });
    expect(await screen.findByRole("row", { name: /Mercado Boa Compra.*Enviado/ })).toBeInTheDocument();
  });

  it("turns a quote into an open order, with the category of the receivable, and points to the sales", async () => {
    const fetchMock = mockApi(
      answer([{ ...quote, status: "approved" }], (url, init) =>
        url.endsWith("/quotes/q1/convert") && init?.method === "POST"
          ? {
              status: 201,
              body: { id: "s1", company_id: "c1", number: 4, quote_id: "q1", status: "open", lines: [], total: "35" },
            }
          : undefined,
      ),
    );
    renderAt("/companies/c1/quotes");
    await userEvent.click(await screen.findByRole("button", { name: "Converter o orçamento 7 em venda" }));
    const region = within(screen.getByRole("region", { name: "Conversão do orçamento 7" }));

    await userEvent.selectOptions(region.getByLabelText("Categoria da receita"), "cat1");
    fireEvent.change(region.getByLabelText("Data da venda"), { target: { value: "2026-11-07" } });
    await userEvent.click(region.getByRole("button", { name: "Criar pedido" }));

    expect(await region.findByText(/Pedido nº 4 criado a partir deste orçamento/)).toBeInTheDocument();
    expect(region.getByRole("link", { name: "Ir para as vendas" })).toHaveAttribute("href", "/companies/c1/sales");
    expect(JSON.parse(String(sentCalls(fetchMock, "POST")[0][1]?.body))).toEqual({
      sold_on: "2026-11-07",
      category_id: "cat1",
    });
    expect(await screen.findByRole("row", { name: /Mercado Boa Compra.*Virou venda/ })).toBeInTheDocument();
  });

  it("offers no way to change or send a quote that was answered, and nothing to write to a role that cannot", async () => {
    mockApi(answer([approvedQuote]));
    renderAt("/companies/c1/quotes");
    await screen.findByRole("row", { name: /Mercado Boa Compra/ });
    expect(screen.queryByRole("button", { name: /Alterar o orçamento/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Enviar o orçamento/ })).not.toBeInTheDocument();
    // An answered quote may still become a sale; a declined one may not.
    expect(screen.getByRole("button", { name: "Converter o orçamento 9 em venda" })).toBeInTheDocument();
  });

  it("shows the list and no form to a role that only reads sales", async () => {
    mockApi(
      answer([quote], (url) =>
        url === "/api/auth/me"
          ? {
              status: 200,
              body: {
                ...profile,
                companies: [
                  { id: "c1", name: "Padaria", role: "viewer", permissions: { ...OWNER_PERMISSIONS, sales: "read" } },
                ],
              },
            }
          : undefined,
      ),
    );
    renderAt("/companies/c1/quotes");
    await screen.findByRole("row", { name: /Mercado Boa Compra/ });
    expect(screen.queryByRole("heading", { name: "Novo orçamento" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Alterar o orçamento/ })).not.toBeInTheDocument();
  });

  it("opens the history of a quote with names instead of ids", async () => {
    mockApi(
      answer([quote], (url) =>
        url.endsWith("/quotes/q1/history")
          ? {
              status: 200,
              body: [
                {
                  sequence: 1,
                  at: 1_700_000_000_000,
                  actor: { id: "u1", name: "Ana" },
                  action: "created",
                  changes: [
                    { field: "customer_id", from: null, to: "p1" },
                    { field: "status", from: null, to: "draft" },
                    { field: "lines", from: null, to: "pr1:3:10.1:0.3:Café: o torrado" },
                  ],
                },
              ],
            }
          : undefined,
      ),
    );
    renderAt("/companies/c1/quotes");
    await userEvent.click(await screen.findByRole("button", { name: "Histórico do orçamento 7" }));
    const history = within(await screen.findByRole("region", { name: "Histórico do orçamento 7" }));
    expect(await history.findByText("Mercado Boa Compra")).toBeInTheDocument();
    expect(history.getByText("Rascunho")).toBeInTheDocument();
    expect(history.getByText("Café: o torrado × 3 a R$ 10,10 (− R$ 0,30)")).toBeInTheDocument();
  });
});

describe("the page a quote's link leads to", () => {
  const publicQuote: PublicQuote = {
    company_name: "Padaria",
    customer_name: "Mercado Boa Compra",
    number: 7,
    issued_on: "2026-11-06",
    valid_until: "2026-11-20",
    expired: false,
    payment_term: "30/60",
    notes: "Entrega em 5 dias",
    status: "sent",
    lines: quote.lines.map(({ product_id: _product, ...line }) => line),
    total: "35",
  };

  const anonymous =
    (body: PublicQuote | null, extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined) =>
    (url: string, init?: RequestInit): Reply => {
      const special = extra(url, init);
      if (special) return special;
      if (url === "/api/auth/me") return { status: 401, body: { error: "authentication required" } };
      if (url === "/api/public/quotes/tok") {
        return body === null ? { status: 404, body: { error: "quote not found" } } : { status: 200, body };
      }
      return { status: 404, body: { error: "not found" } };
    };

  it("shows the quote with no account, and approves it once", async () => {
    const fetchMock = mockApi(
      anonymous(publicQuote, (url, init) =>
        url === "/api/public/quotes/tok/approve" && init?.method === "POST"
          ? { status: 200, body: { ...publicQuote, status: "approved" } }
          : undefined,
      ),
    );
    renderAt("/quote/tok");

    expect(await screen.findByRole("heading", { name: "Orçamento nº 7" })).toBeInTheDocument();
    expect(screen.getByText("Padaria para Mercado Boa Compra")).toBeInTheDocument();
    expect(screen.getByText(/válido até 20\/11\/2026 · pagamento: 30\/60/)).toBeInTheDocument();
    const rows = screen.getAllByRole("row");
    expect(rows[1]).toHaveTextContent("Café 500g");
    expect(rows[1]).toHaveTextContent("R$ 10,10");
    expect(rows[1]).toHaveTextContent("R$ 0,30");
    expect(rows[2]).toHaveTextContent("1,5");
    expect(screen.getByRole("link", { name: /Baixar em PDF/ })).toHaveAttribute("href", "/api/public/quotes/tok/pdf");

    await userEvent.click(screen.getByRole("button", { name: "Aprovar orçamento" }));
    expect(await screen.findByText("Orçamento aprovado. Obrigado!")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Aprovar orçamento" })).not.toBeInTheDocument();
    expect(fetchMock.mock.calls.some(([url]) => String(url).endsWith("/approve"))).toBe(true);
  });

  it("lets the customer decline", async () => {
    mockApi(
      anonymous(publicQuote, (url, init) =>
        url === "/api/public/quotes/tok/decline" && init?.method === "POST"
          ? { status: 200, body: { ...publicQuote, status: "declined" } }
          : undefined,
      ),
    );
    renderAt("/quote/tok");
    await userEvent.click(await screen.findByRole("button", { name: "Recusar" }));
    expect(await screen.findByText("Situação: Recusado.")).toBeInTheDocument();
  });

  it("offers no answer to a quote past its validity or already answered, and explains a link that leads nowhere", async () => {
    mockApi(anonymous({ ...publicQuote, expired: true }));
    const first = renderAt("/quote/tok");
    expect(await screen.findByText(/A validade deste orçamento acabou/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Aprovar orçamento" })).not.toBeInTheDocument();
    // Declining is still possible.
    expect(screen.getByRole("button", { name: "Recusar" })).toBeInTheDocument();
    first.unmount();

    mockApi(anonymous({ ...publicQuote, status: "declined" }));
    const second = renderAt("/quote/tok");
    expect(await screen.findByText("Situação: Recusado.")).toBeInTheDocument();
    second.unmount();

    mockApi(anonymous(null));
    renderAt("/quote/tok");
    expect(await screen.findByText(/Este link não leva a nenhum orçamento/)).toBeInTheDocument();
  });

  it("says why an answer was refused", async () => {
    mockApi(
      anonymous(publicQuote, (url, init) =>
        url.endsWith("/approve") && init?.method === "POST"
          ? { status: 410, body: { error: "the quote is past its validity" } }
          : undefined,
      ),
    );
    renderAt("/quote/tok");
    await userEvent.click(await screen.findByRole("button", { name: "Aprovar orçamento" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("the quote is past its validity");
  });
});
