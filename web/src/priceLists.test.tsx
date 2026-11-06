import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Party, PriceList, Product } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const product = (id: string, name: string, sale_price: string | null, active = true): Product => ({
  id,
  company_id: "c1",
  kind: "product",
  name,
  sku: null,
  unit: "UN",
  ncm: null,
  sale_price,
  cost_price: null,
  minimum_stock: null,
  active,
});
const coffee = product("pr1", "Café 500g", "10.1");
const bread = product("pr2", "Pão francês", "3.33");
const old = product("pr3", "Descontinuado", "1", false);

const wholesale: PriceList = {
  id: "l1",
  company_id: "c1",
  name: "Atacado",
  items: [{ product_id: "pr1", price: "8.5" }],
  active: true,
};
const retail: PriceList = { ...wholesale, id: "l2", name: "Varejo", items: [{ product_id: "pr1", price: "12" }, { product_id: "pr2", price: "4" }], active: false };

const calls = (fetchMock: ReturnType<typeof mockApi>, method: string) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === method);

const answer =
  (lists: PriceList[], extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined) =>
  (url: string, init?: RequestInit): Reply => {
    const special = extra(url, init);
    if (special) return special;
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.includes("/products")) return { status: 200, body: { items: [coffee, bread, old], next_cursor: null } };
    if (url.includes("/price-lists")) return { status: 200, body: { items: lists, next_cursor: null } };
    return { status: 404, body: { error: "not found" } };
  };

const formOf = async (heading: string) => {
  // A new list is asked for with the button; a change opens the sheet by itself.
  if (heading === "Nova tabela de preço") {
    await userEvent.click(await screen.findByRole("button", { name: heading }));
  }
  const aside = (await screen.findByRole("heading", { name: heading })).closest("aside")!;
  const form = within(aside);
  await form.findByLabelText("Nome");
  return form;
};

describe("price lists", () => {
  it("lists the price lists with how many prices they hold and whether they are in use", async () => {
    mockApi(answer([wholesale, retail]));
    renderAt("/companies/c1/price-lists");

    const first = await screen.findByRole("row", { name: /Atacado/ });
    expect(first).toHaveTextContent("1");
    expect(first).toHaveTextContent("Ativa");
    const second = screen.getByRole("row", { name: /Varejo/ });
    expect(second).toHaveTextContent("2");
    expect(second).toHaveTextContent("Inativa");
  });

  it("registers a list: each product once, prices as typed, and every product can be brought in at once", async () => {
    const fetchMock = mockApi(
      answer([], (url, init) =>
        url.endsWith("/price-lists") && init?.method === "POST" ? { status: 201, body: wholesale } : undefined,
      ),
    );
    renderAt("/companies/c1/price-lists");
    const form = await formOf("Nova tabela de preço");

    await userEvent.type(form.getByLabelText("Nome"), "Atacado");
    await userEvent.click(form.getByRole("button", { name: "Incluir todos os produtos" }));
    // Both active products come with their sale price; the discontinued one is not offered.
    expect(form.getByLabelText("Preço do item 1")).toHaveValue("10,1");
    expect(form.getByLabelText("Preço do item 2")).toHaveValue("3,33");
    expect(within(form.getByLabelText("Produto ou serviço do preço 1")).queryByRole("option", { name: "Descontinuado" })).toBeNull();
    await userEvent.clear(form.getByLabelText("Preço do item 1"));
    await userEvent.type(form.getByLabelText("Preço do item 1"), "8,50");
    await userEvent.click(form.getByRole("button", { name: "Cadastrar tabela" }));

    expect(await screen.findByText("Atacado cadastrada.")).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({
      name: "Atacado",
      items: [
        { product_id: "pr1", price: "8.50" },
        { product_id: "pr2", price: "3.33" },
      ],
      active: true,
    });
  });

  it("refuses a product twice and a price that is not a number before asking the server", async () => {
    const fetchMock = mockApi(answer([]));
    renderAt("/companies/c1/price-lists");
    const form = await formOf("Nova tabela de preço");

    await userEvent.type(form.getByLabelText("Nome"), "Atacado");
    await userEvent.selectOptions(form.getByLabelText("Produto ou serviço do preço 1"), "pr1");
    await userEvent.clear(form.getByLabelText("Preço do item 1"));
    await userEvent.type(form.getByLabelText("Preço do item 1"), "1.050");
    await userEvent.click(form.getByRole("button", { name: "Cadastrar tabela" }));
    expect(await form.findByRole("alert")).toHaveTextContent("Informe o preço do item 1");

    await userEvent.clear(form.getByLabelText("Preço do item 1"));
    await userEvent.type(form.getByLabelText("Preço do item 1"), "9");
    await userEvent.click(form.getByRole("button", { name: "Adicionar produto" }));
    // A product already in the list cannot be picked again.
    expect(within(form.getByLabelText("Produto ou serviço do preço 2")).getByRole("option", { name: "Café 500g" })).toBeDisabled();
    expect(calls(fetchMock, "POST")).toHaveLength(0);
  });

  it("edits a list, replacing it as a whole, and shows its trail with product names and prices", async () => {
    let stored = wholesale;
    const fetchMock = mockApi(
      answer([], (url, init) => {
        if (url.endsWith("/price-lists/l1") && init?.method === "PUT") {
          stored = { ...wholesale, items: [{ product_id: "pr2", price: "2.75" }], active: false };
          return { status: 200, body: stored };
        }
        if (url.endsWith("/price-lists/l1/history")) {
          return {
            status: 200,
            body: [
              {
                at: Date.parse("2026-11-06T12:00:00Z"),
                actor: { kind: "user", id: "u1", name: "Ana" },
                action: "updated",
                changes: [{ field: "items", from: "pr1:10.5", to: "pr1:8.5\npr2:3" }],
              },
            ],
          };
        }
        return url.endsWith("/price-lists") && !init?.method ? { status: 200, body: { items: [stored], next_cursor: null } } : undefined;
      }),
    );
    renderAt("/companies/c1/price-lists");
    await userEvent.click(await screen.findByRole("button", { name: "Alterar Atacado" }));
    const form = within(screen.getByRole("heading", { name: "Alterar Atacado" }).closest("aside")!);

    expect(form.getByLabelText("Preço do item 1")).toHaveValue("8,5");
    await userEvent.selectOptions(form.getByLabelText("Produto ou serviço do preço 1"), "pr2");
    await userEvent.clear(form.getByLabelText("Preço do item 1"));
    await userEvent.type(form.getByLabelText("Preço do item 1"), "2,75");
    await userEvent.click(form.getByLabelText("Ativa"));
    await userEvent.click(form.getByRole("button", { name: "Salvar alterações" }));

    expect(await screen.findByText(/Atacado atualizada/)).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "PUT")[0][1]?.body))).toEqual({
      name: "Atacado",
      items: [{ product_id: "pr2", price: "2.75" }],
      active: false,
    });

    await userEvent.click(await screen.findByRole("button", { name: "Histórico de Atacado" }));
    const trail = within(await screen.findByRole("region", { name: "Histórico de Atacado" }));
    expect(await trail.findByText(/Café 500g a R\$ 10,50 → Café 500g a R\$ 8,50; Pão francês a R\$ 3,00/)).toBeInTheDocument();
  });

  it("shows the lists and nothing to write to a role that only reads sales", async () => {
    mockApi(
      answer([wholesale], (url) =>
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
    renderAt("/companies/c1/price-lists");
    await screen.findByRole("row", { name: /Atacado/ });
    expect(screen.queryByRole("button", { name: "Nova tabela de preço" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Alterar/ })).not.toBeInTheDocument();
  });
});

describe("choosing a price list in a document", () => {
  const customer: Party = {
    id: "p1",
    company_id: "c1",
    name: "Mercado Boa Compra",
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
  };
  const revenue = { id: "cat1", company_id: "c1", parent_id: null, name: "Vendas", dre_group: "gross_revenue", cash_flow_group: "operating", level: 1, active: true };
  const documents = (url: string): Reply | undefined => {
    if (url.includes("/parties")) return { status: 200, body: { items: [customer], next_cursor: null } };
    if (url.includes("/payment-terms")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [revenue] } };
    if (url.includes("/cost-centers")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.includes("/contracts")) return { status: 200, body: { items: [], next_cursor: null } };
    return undefined;
  };

  it("fills the price of a new line from the list, falls back to the product's, and prices the lines again when the list changes", async () => {
    mockApi(answer([wholesale], documents));
    renderAt("/companies/c1/contracts");
    const form = within((await screen.findByRole("heading", { name: "Novo contrato" })).closest("aside")!);
    await form.findByLabelText("Cliente");

    await userEvent.selectOptions(form.getByLabelText("Produto ou serviço do item 1"), "pr1");
    expect(form.getByLabelText("Preço do item 1")).toHaveValue("10,1");

    // The list names coffee: the line that is already there takes its price.
    await userEvent.selectOptions(await form.findByLabelText("Tabela de preços"), "l1");
    expect(form.getByLabelText("Preço do item 1")).toHaveValue("8,5");

    // A product the list does not name keeps the product's own price.
    await userEvent.click(form.getByRole("button", { name: "Adicionar item" }));
    await userEvent.selectOptions(form.getByLabelText("Produto ou serviço do item 2"), "pr2");
    expect(form.getByLabelText("Preço do item 2")).toHaveValue("3,33");

    // Going back to no list prices every line by its product again.
    await userEvent.selectOptions(form.getByLabelText("Tabela de preços"), "");
    expect(form.getByLabelText("Preço do item 1")).toHaveValue("10,1");
  });

  it("does not offer a choice when the company has no price list", async () => {
    mockApi(answer([], documents));
    renderAt("/companies/c1/contracts");
    const form = within((await screen.findByRole("heading", { name: "Novo contrato" })).closest("aside")!);
    await form.findByLabelText("Cliente");

    expect(form.queryByLabelText("Tabela de preços")).not.toBeInTheDocument();
  });
});
