import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Contract, Party, Product } from "./api";
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
  kind: "service",
  name: "Suporte",
  sku: null,
  unit: "UN",
  ncm: null,
  sale_price: "100.1",
  cost_price: null,
  minimum_stock: null,
  active: true,
};
const revenue = { id: "cat1", company_id: "c1", parent_id: null, name: "Vendas", dre_group: "gross_revenue", cash_flow_group: "operating", level: 1, active: true };

const monthly: Contract = {
  id: "k1",
  company_id: "c1",
  name: "Manutenção mensal",
  customer_id: "p1",
  seller_id: null,
  payment_term_id: null,
  category_id: "cat1",
  cost_center_id: null,
  notes: null,
  lines: [{ product_id: "pr1", description: "Suporte", quantity: "2", unit_price: "100.1", discount: "0.2", total: "200" }],
  total: "200",
  cycle: "monthly",
  starts_on: "2026-11-05",
  ends_on: null,
  status: "active",
  billed_through: "2026-12-05",
};
const ended: Contract = { ...monthly, id: "k2", name: "Antigo", status: "terminated", ends_on: "2026-12-31", cycle: "quarterly" };

const calls = (fetchMock: ReturnType<typeof mockApi>, method: string) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === method);

const answer =
  (items: Contract[], extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined) =>
  (url: string, init?: RequestInit): Reply => {
    const special = extra(url, init);
    if (special) return special;
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.includes("/parties")) return { status: 200, body: { items: [party("p1", "Mercado Boa Compra", false), party("p2", "Vera", true)], next_cursor: null } };
    if (url.includes("/products")) return { status: 200, body: { items: [coffee], next_cursor: null } };
    if (url.includes("/payment-terms")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [revenue] } };
    if (url.includes("/cost-centers")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.includes("/contracts")) return { status: 200, body: { items, next_cursor: null } };
    return { status: 404, body: { error: "not found" } };
  };

describe("contracts", () => {
  it("lists the contracts with their customer, cycle, what each cycle bills and how far they were billed", async () => {
    mockApi(answer([monthly, ended]));
    renderAt("/companies/c1/contracts");

    const row = await screen.findByRole("row", { name: /Manutenção mensal/ });
    expect(row).toHaveTextContent("Mercado Boa Compra");
    expect(row).toHaveTextContent("Todo mês");
    expect(row).toHaveTextContent("desde 05/11/2026");
    expect(row).toHaveTextContent("R$ 200,00");
    expect(row).toHaveTextContent("Ativo");
    expect(row).toHaveTextContent("cobrado até 05/12/2026");
    const other = screen.getByRole("row", { name: /Antigo/ });
    expect(other).toHaveTextContent("A cada 3 meses");
    expect(other).toHaveTextContent("Encerrado");
    // A terminated contract can only be read.
    expect(within(other).queryByRole("button", { name: /Alterar|Reajustar|Encerrar/ })).not.toBeInTheDocument();
    expect(within(row).getByRole("button", { name: "Reajustar Manutenção mensal" })).toBeInTheDocument();
  });

  it("registers a contract and says that the cycles already due were billed", async () => {
    const fetchMock = mockApi(
      answer([], (url, init) =>
        url.endsWith("/contracts") && init?.method === "POST" ? { status: 201, body: monthly } : undefined,
      ),
    );
    renderAt("/companies/c1/contracts");
    const form = within((await screen.findByRole("heading", { name: "Novo contrato" })).closest("aside")!);
    await form.findByLabelText("Cliente");

    await userEvent.type(form.getByLabelText("Nome"), "Manutenção mensal");
    await userEvent.selectOptions(form.getByLabelText("Cliente"), "p1");
    await userEvent.selectOptions(form.getByLabelText("Categoria da receita"), "cat1");
    await userEvent.selectOptions(form.getByLabelText("Produto ou serviço do item 1"), "pr1");
    await userEvent.clear(form.getByLabelText("Quantidade do item 1"));
    await userEvent.type(form.getByLabelText("Quantidade do item 1"), "2");
    await userEvent.type(form.getByLabelText("Desconto do item 1"), "0,20");
    fireEvent.change(form.getByLabelText("Primeira cobrança"), { target: { value: "2026-11-05" } });
    await userEvent.click(form.getByRole("button", { name: "Cadastrar contrato" }));

    expect(await screen.findByText(/Manutenção mensal cadastrado: os ciclos que já venceram foram cobrados/)).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({
      name: "Manutenção mensal",
      customer_id: "p1",
      category_id: "cat1",
      lines: [{ product_id: "pr1", description: "Suporte", quantity: "2", unit_price: "100.1", discount: "0.20" }],
      cycle: "monthly",
      starts_on: "2026-11-05",
      active: true,
    });
  });

  it("adjusts the price by a percentage and says what each cycle bills now", async () => {
    const fetchMock = mockApi(
      answer([monthly], (url, init) =>
        url.endsWith("/contracts/k1/adjust") && init?.method === "POST"
          ? { status: 200, body: { ...monthly, total: "220.02" } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/contracts");
    await userEvent.click(await screen.findByRole("button", { name: "Reajustar Manutenção mensal" }));
    const region = within(screen.getByRole("region", { name: "Reajuste de Manutenção mensal" }));

    await userEvent.type(region.getByLabelText("Reajuste (%)"), "10");
    await userEvent.click(region.getByRole("button", { name: "Aplicar reajuste" }));

    expect(await screen.findByText(/cada ciclo agora cobra R\$ 220,02, a partir do próximo/)).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({ percent: "10" });
  });

  it("terminates a contract for good, saying so, and shows why when it is refused", async () => {
    let refuse = true;
    mockApi(
      answer([monthly], (url, init) => {
        if (!url.endsWith("/contracts/k1/terminate") || init?.method !== "POST") return undefined;
        if (refuse) {
          refuse = false;
          return { status: 422, body: { error: "ends_on must not be before starts_on" } };
        }
        return { status: 200, body: { ...monthly, status: "terminated", ends_on: "2027-03-31" } };
      }),
    );
    renderAt("/companies/c1/contracts");
    await userEvent.click(await screen.findByRole("button", { name: "Encerrar Manutenção mensal" }));
    const region = within(screen.getByRole("region", { name: "Encerramento de Manutenção mensal" }));

    expect(region.getByText(/Encerrar não se desfaz/)).toBeInTheDocument();
    fireEvent.change(region.getByLabelText("Último dia de cobrança"), { target: { value: "2027-03-31" } });
    await userEvent.click(region.getByRole("button", { name: "Encerrar contrato" }));
    expect(await region.findByRole("alert")).toHaveTextContent("ends_on must not be before starts_on");
    await userEvent.click(region.getByRole("button", { name: "Encerrar contrato" }));
    expect(await screen.findByText(/Manutenção mensal encerrado: nada será cobrado depois de 31\/03\/2027/)).toBeInTheDocument();
    expect(await screen.findByRole("row", { name: /Manutenção mensal.*Encerrado/ })).toBeInTheDocument();
  });

  it("shows the list and nothing to write to a role that only reads sales", async () => {
    mockApi(
      answer([monthly], (url) =>
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
    renderAt("/companies/c1/contracts");
    await screen.findByRole("row", { name: /Manutenção mensal/ });
    expect(screen.queryByRole("heading", { name: "Novo contrato" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Reajustar|Encerrar/ })).not.toBeInTheDocument();
  });
});
