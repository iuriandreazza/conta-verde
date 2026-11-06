import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { CommissionRule, CommissionStatement, Party, Product } from "./api";
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

const general: CommissionRule = {
  id: "r1",
  company_id: "c1",
  seller_id: null,
  product_id: null,
  rate: "5",
  starts_on: "2026-01-01",
  ends_on: null,
  active: true,
};
const special: CommissionRule = { ...general, id: "r2", seller_id: "s1", product_id: "pr1", rate: "7.5", ends_on: "2026-12-31" };

const statement: CommissionStatement = {
  from: "2026-11-01",
  to: "2026-11-30",
  base: "35",
  commission: "2.65",
  sellers: [
    {
      seller_id: "s1",
      base: "35",
      commission: "2.65",
      sales: [{ sale_id: "v1", number: 7, sold_on: "2026-11-05", customer_id: "p1", base: "35", commission: "2.65" }],
    },
  ],
};

const calls = (fetchMock: ReturnType<typeof mockApi>, method: string) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === method);

const answer =
  (rules: CommissionRule[], extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined) =>
  (url: string, init?: RequestInit): Reply => {
    const special = extra(url, init);
    if (special) return special;
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.includes("/parties")) return { status: 200, body: { items: [party("p1", "Mercado Boa Compra", false), party("s1", "Vera", true)], next_cursor: null } };
    if (url.includes("/products")) return { status: 200, body: { items: [coffee], next_cursor: null } };
    if (url.includes("/commission-rules")) return { status: 200, body: { items: rules, next_cursor: null } };
    if (url.includes("/commissions?")) return { status: 200, body: statement };
    return { status: 404, body: { error: "not found" } };
  };

describe("commissions", () => {
  it("shows what each seller earned, sale by sale, with the totals", async () => {
    const fetchMock = mockApi(answer([general]));
    renderAt("/companies/c1/commissions");

    const card = within(await screen.findByRole("region", { name: "Extrato de comissões" }));
    const seller = await card.findByRole("row", { name: /Vera/ });
    expect(seller).toHaveTextContent("R$ 35,00");
    expect(seller).toHaveTextContent("R$ 2,65");
    expect(card.getByRole("row", { name: /Venda 7/ })).toHaveTextContent("05/11/2026 · Mercado Boa Compra");
    expect(card.getByRole("row", { name: /Total/ })).toHaveTextContent("R$ 2,65");
    const asked = fetchMock.mock.calls.map(([url]) => String(url)).find((url) => url.includes("/commissions?"));
    expect(asked).toMatch(/from=\d{4}-\d{2}-01&to=\d{4}-\d{2}-\d{2}$/);
  });

  it("asks again for one seller and says so when nobody earned anything", async () => {
    const fetchMock = mockApi(
      answer([general], (url) =>
        url.includes("/commissions?") && url.includes("seller=s1")
          ? { status: 200, body: { ...statement, base: "0", commission: "0", sellers: [] } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/commissions");
    const card = within(await screen.findByRole("region", { name: "Extrato de comissões" }));
    await card.findByRole("row", { name: /Venda 7/ });

    await userEvent.selectOptions(card.getByLabelText("Vendedor"), "s1");

    expect(await card.findByText("Nenhuma comissão neste período.")).toBeInTheDocument();
    expect(fetchMock.mock.calls.some(([url]) => String(url).includes("seller=s1"))).toBe(true);
  });

  it("refuses a span that ends before it starts without asking the server", async () => {
    const fetchMock = mockApi(answer([general]));
    renderAt("/companies/c1/commissions");
    const card = within(await screen.findByRole("region", { name: "Extrato de comissões" }));
    await card.findByRole("row", { name: /Venda 7/ });
    const before = fetchMock.mock.calls.length;

    fireEvent.change(card.getByLabelText("De"), { target: { value: "2099-01-01" } });

    expect(await card.findByText(/o primeiro dia não venha depois do último/)).toBeInTheDocument();
    expect(fetchMock.mock.calls.length).toBe(before);
  });

  it("lists the rules with who and what they cover", async () => {
    mockApi(answer([general, special]));
    renderAt("/companies/c1/commissions");

    const rules = within(await screen.findByRole("region", { name: "Regras de comissão" }));
    const first = await rules.findByRole("row", { name: /Todos os vendedores/ });
    expect(first).toHaveTextContent("Todos os produtos");
    expect(first).toHaveTextContent("5%");
    expect(first).toHaveTextContent("desde 01/01/2026");
    const second = rules.getByRole("row", { name: /Vera/ });
    expect(second).toHaveTextContent("Café 500g");
    expect(second).toHaveTextContent("7,5%");
    expect(second).toHaveTextContent("até 31/12/2026");
    expect(second).toHaveTextContent("Ativa");
  });

  it("registers a rule for a seller and a product", async () => {
    const created: CommissionRule = { ...special, ends_on: null };
    const fetchMock = mockApi(
      answer([], (url, init) =>
        url.endsWith("/commission-rules") && init?.method === "POST" ? { status: 201, body: created } : undefined,
      ),
    );
    renderAt("/companies/c1/commissions");
    const form = within((await screen.findByRole("heading", { name: "Nova regra" })).closest("aside")!);
    await form.findByLabelText("Vendedor");

    await userEvent.selectOptions(form.getByLabelText("Vendedor"), "s1");
    await userEvent.selectOptions(form.getByLabelText("Produto"), "pr1");
    await userEvent.type(form.getByLabelText("Percentual (%)"), "7,5");
    fireEvent.change(form.getByLabelText("Vale a partir de"), { target: { value: "2026-01-01" } });
    await userEvent.click(form.getByRole("button", { name: "Cadastrar regra" }));

    expect(await screen.findByText("Regra de 7,5% cadastrada.")).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({
      seller_id: "s1",
      product_id: "pr1",
      rate: "7.5",
      starts_on: "2026-01-01",
      active: true,
    });
  });

  it("refuses an ambiguous percentage and shows the server's refusal", async () => {
    mockApi(
      answer([], (url, init) =>
        url.endsWith("/commission-rules") && init?.method === "POST"
          ? { status: 422, body: { error: "rate must be above 0 and at most 100" } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/commissions");
    const form = within((await screen.findByRole("heading", { name: "Nova regra" })).closest("aside")!);
    await form.findByLabelText("Percentual (%)");

    await userEvent.type(form.getByLabelText("Percentual (%)"), "1.050");
    await userEvent.click(form.getByRole("button", { name: "Cadastrar regra" }));
    expect(await form.findByRole("alert")).toHaveTextContent("Informe o percentual");

    await userEvent.clear(form.getByLabelText("Percentual (%)"));
    await userEvent.type(form.getByLabelText("Percentual (%)"), "150");
    await userEvent.click(form.getByRole("button", { name: "Cadastrar regra" }));
    expect(await form.findByRole("alert")).toHaveTextContent("rate must be above 0 and at most 100");
  });

  it("edits a rule, replacing it as a whole", async () => {
    let stored = special;
    const fetchMock = mockApi(
      answer([], (url, init) => {
        if (url.endsWith("/commission-rules/r2") && init?.method === "PUT") {
          stored = { ...special, rate: "8", active: false };
          return { status: 200, body: stored };
        }
        return url.includes("/commission-rules") && !init?.method ? { status: 200, body: { items: [stored], next_cursor: null } } : undefined;
      }),
    );
    renderAt("/companies/c1/commissions");
    await userEvent.click(await screen.findByRole("button", { name: "Alterar a regra de 7,5% desde 01/01/2026" }));
    const form = within(screen.getByRole("heading", { name: "Alterar regra" }).closest("aside")!);

    expect(form.getByLabelText("Percentual (%)")).toHaveValue("7,5");
    await userEvent.clear(form.getByLabelText("Percentual (%)"));
    await userEvent.type(form.getByLabelText("Percentual (%)"), "8");
    await userEvent.click(form.getByLabelText("Ativa"));
    await userEvent.click(form.getByRole("button", { name: "Salvar alterações" }));

    expect(await screen.findByText(/Regra de 8% atualizada/)).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "PUT")[0][1]?.body))).toEqual({
      seller_id: "s1",
      product_id: "pr1",
      rate: "8",
      starts_on: "2026-01-01",
      ends_on: "2026-12-31",
      active: false,
    });
    expect(await screen.findByRole("row", { name: /8%.*Inativa/ })).toBeInTheDocument();
  });

  it("keeps the seller of a rule in the choices when editing, even if they stopped being a seller", async () => {
    mockApi(
      answer([special], (url) =>
        url.includes("/parties")
          ? { status: 200, body: { items: [party("p1", "Mercado Boa Compra", false), party("s1", "Vera", false)], next_cursor: null } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/commissions");
    await userEvent.click(await screen.findByRole("button", { name: "Alterar a regra de 7,5% desde 01/01/2026" }));
    const form = within(screen.getByRole("heading", { name: "Alterar regra" }).closest("aside")!);

    expect(form.getByLabelText("Vendedor")).toHaveValue("s1");
  });

  it("shows the trail of a rule in words", async () => {
    mockApi(
      answer([special], (url) =>
        url.endsWith("/commission-rules/r2/history")
          ? {
              status: 200,
              body: [
                {
                  at: Date.parse("2026-11-06T12:00:00Z"),
                  actor: { kind: "user", id: "u1", name: "Ana" },
                  action: "updated",
                  changes: [
                    { field: "rate", from: "5", to: "7.5" },
                    { field: "active", from: "true", to: "false" },
                  ],
                },
              ],
            }
          : undefined,
      ),
    );
    renderAt("/companies/c1/commissions");
    await userEvent.click(await screen.findByRole("button", { name: "Histórico da regra de 7,5% desde 01/01/2026" }));

    const region = within(await screen.findByRole("region", { name: "Histórico da regra de 7,5%" }));
    expect(await region.findByText(/Percentual/)).toBeInTheDocument();
    expect(region.getByText(/5% → 7,5%/)).toBeInTheDocument();
    expect(region.getByText(/Ativa → Inativa/)).toBeInTheDocument();
  });

  it("shows the statement and the rules, and nothing to write, to a role that only reads sales", async () => {
    mockApi(
      answer([general], (url) =>
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
    renderAt("/companies/c1/commissions");
    await screen.findByRole("row", { name: /Todos os vendedores/ });
    expect(screen.queryByRole("heading", { name: "Nova regra" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Alterar a regra/ })).not.toBeInTheDocument();
    expect(screen.getByRole("row", { name: /Venda 7/ })).toBeInTheDocument();
  });
});
