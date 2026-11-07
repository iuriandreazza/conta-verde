import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { InventoryCount, Product } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const product = (id: string, name: string, patch: Partial<Product> = {}): Product => ({
  id,
  company_id: "c1",
  kind: "product",
  name,
  sku: null,
  unit: "UN",
  ncm: null,
  sale_price: null,
  cost_price: null,
  minimum_stock: null,
  active: true,
  ...patch,
});
const beans = product("pr1", "Grão de café 1kg", { sku: "grao" });
const filters = product("pr2", "Filtro de papel");
const sugar = product("pr3", "Açúcar");

const count = (patch: Partial<InventoryCount>): InventoryCount => ({
  id: "i1",
  company_id: "c1",
  number: 1,
  counted_on: "2026-11-10",
  notes: null,
  status: "open",
  lines: [
    { product_id: "pr1", counted: "8", expected: null, difference: null },
    { product_id: "pr2", counted: "3", expected: null, difference: null },
  ],
  ...patch,
});
const open = count({});
const applied = count({ id: "i2", number: 2, status: "applied", notes: "Fim de semana" });
const cancelled = count({ id: "i3", number: 3, status: "cancelled" });

const calls = (fetchMock: ReturnType<typeof mockApi>, method: string) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === method);

const answer =
  (items: InventoryCount[], extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined) =>
  (url: string, init?: RequestInit): Reply => {
    const special = extra(url, init);
    if (special) return special;
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.includes("/products")) return { status: 200, body: { items: [beans, filters, sugar], next_cursor: null } };
    if (url.includes("/inventory-counts")) return { status: 200, body: { items, next_cursor: null } };
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

const openSheet = async () => {
  await userEvent.click(await screen.findByRole("button", { name: "Novo inventário" }));
  const sheet = within((await screen.findByRole("heading", { name: "Novo inventário" })).closest("aside")!);
  await sheet.findByLabelText("Produto do item 1");
  return sheet;
};

describe("inventory counts", () => {
  it("lists the counts with their day, notes, number of products and status in words", async () => {
    mockApi(answer([open, applied, cancelled]));
    renderAt("/companies/c1/inventory-counts");

    const row = (await screen.findAllByRole("row", { name: /10\/11\/2026/ }))[0];
    expect(row).toHaveTextContent("1");
    expect(row).toHaveTextContent("2 produtos");
    expect(row).toHaveTextContent("Aberto");
    expect(screen.getByRole("row", { name: /Fim de semana.*Aplicado/ })).toHaveTextContent("2");
    expect(screen.getByRole("row", { name: /Cancelado/ })).toHaveTextContent("3");
    // Only an open count can be changed, applied or cancelled.
    expect(screen.getByRole("button", { name: "Alterar o inventário 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Aplicar o inventário 1" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Aplicar o inventário 2" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancelar o inventário 3" })).not.toBeInTheDocument();
  });

  it("creates a count from the products it loads, dropping the ones left blank, with comma quantities", async () => {
    const fetchMock = mockApi(
      answer([], (url, init) => (url.endsWith("/inventory-counts") && init?.method === "POST" ? { status: 201, body: open } : undefined)),
    );
    renderAt("/companies/c1/inventory-counts");
    const sheet = await openSheet();

    await userEvent.click(sheet.getByRole("button", { name: "Carregar todos os produtos" }));
    expect(sheet.getByLabelText("Produto do item 1")).toHaveValue("pr1");
    expect(sheet.getByLabelText("Produto do item 2")).toHaveValue("pr2");
    expect(sheet.getByLabelText("Produto do item 3")).toHaveValue("pr3");
    expect(sheet.getByRole("button", { name: "Carregar todos os produtos" })).toBeDisabled();
    await userEvent.type(sheet.getByLabelText("Quantidade contada do item 1"), "8");
    await userEvent.type(sheet.getByLabelText("Quantidade contada do item 2"), "2,5");
    // The sugar is left blank: it is not counted.
    fireEvent.change(sheet.getByLabelText("Data da contagem"), { target: { value: "2026-11-10" } });
    await userEvent.type(sheet.getByLabelText("Observações"), "Mensal");
    await userEvent.click(sheet.getByRole("button", { name: "Criar inventário" }));

    expect(await screen.findByText(/Inventário nº 1 criado/)).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({
      counted_on: "2026-11-10",
      notes: "Mensal",
      lines: [
        { product_id: "pr1", counted: "8" },
        { product_id: "pr2", counted: "2.5" },
      ],
    });
  });

  it("starts a count from a spreadsheet: the lines come from the file, what it cannot use is said, and the form still checks them", async () => {
    const fetchMock = mockApi(
      answer([], (url, init) => (url.endsWith("/inventory-counts") && init?.method === "POST" ? { status: 201, body: open } : undefined)),
    );
    renderAt("/companies/c1/inventory-counts");
    await userEvent.click((await screen.findAllByRole("button", { name: "Novo inventário via planilha" }))[0]);
    const sheet = within((await screen.findByRole("heading", { name: "Novo inventário via planilha" })).closest("aside")!);

    // The lines wait for the file; they can also be typed.
    expect(sheet.queryByLabelText("Produto do item 1")).not.toBeInTheDocument();
    expect(sheet.getByRole("button", { name: "Baixar modelo" })).toBeInTheDocument();
    const file = new File(["SKU;Contado\ngrao;8\nNADA;3\n"], "contagem.csv", { type: "text/csv" });
    await userEvent.upload(sheet.getByLabelText("Arquivo da planilha de contagem"), file);

    expect(await sheet.findByLabelText("Produto do item 1")).toHaveValue("pr1");
    expect(sheet.getByLabelText("Quantidade contada do item 1")).toHaveValue("8");
    expect(sheet.getByText(/Não foi possível usar uma linha da planilha.*Linha 3: não há produto com o SKU "NADA"/)).toBeInTheDocument();
    await userEvent.click(sheet.getByRole("button", { name: "Criar inventário" }));

    expect(await screen.findByText(/Inventário nº 1 criado/)).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body)).lines).toEqual([{ product_id: "pr1", counted: "8" }]);
  });

  it("offers the usual form to a count that is typed by hand, with the sheet as an option", async () => {
    mockApi(answer([]));
    renderAt("/companies/c1/inventory-counts");
    const sheet = await openSheet();
    expect(sheet.getByLabelText("Arquivo da planilha de contagem")).toBeInTheDocument();
    expect(sheet.queryByRole("button", { name: "Preencher à mão" })).not.toBeInTheDocument();
  });

  it("refuses a count with nothing counted, an ambiguous quantity and a negative one, before asking the API", async () => {
    const fetchMock = mockApi(answer([]));
    renderAt("/companies/c1/inventory-counts");
    const sheet = await openSheet();

    await userEvent.click(sheet.getByRole("button", { name: "Criar inventário" }));
    expect(await sheet.findByRole("alert")).toHaveTextContent("Informe a quantidade contada de pelo menos um produto");
    await userEvent.selectOptions(sheet.getByLabelText("Produto do item 1"), "pr1");
    await userEvent.type(sheet.getByLabelText("Quantidade contada do item 1"), "1.500");
    await userEvent.click(sheet.getByRole("button", { name: "Criar inventário" }));
    expect(await sheet.findByRole("alert")).toHaveTextContent("ambígua");
    await userEvent.clear(sheet.getByLabelText("Quantidade contada do item 1"));
    await userEvent.type(sheet.getByLabelText("Quantidade contada do item 1"), "-1");
    await userEvent.click(sheet.getByRole("button", { name: "Criar inventário" }));
    expect(await sheet.findByRole("alert")).toHaveTextContent("zero ou mais");
    expect(calls(fetchMock, "POST")).toHaveLength(0);
  });

  it("shows what the stock held and the difference of each line, marked when it is not zero", async () => {
    mockApi(
      answer([open], (url) =>
        url.endsWith("/inventory-counts/i1")
          ? {
              status: 200,
              body: count({
                lines: [
                  { product_id: "pr1", counted: "8", expected: "10", difference: "-2" },
                  { product_id: "pr2", counted: "3", expected: "3", difference: "0" },
                  { product_id: "pr3", counted: "1.5", expected: "1", difference: "0.5" },
                ],
              }),
            }
          : undefined,
      ),
    );
    renderAt("/companies/c1/inventory-counts");
    await userEvent.click(await screen.findByRole("button", { name: "Detalhes do inventário 1" }));
    const region = within(screen.getByRole("region", { name: "Detalhes do inventário 1" }));

    const short = await region.findByRole("row", { name: /Grão de café 1kg/ });
    expect(within(short).getAllByRole("cell").map((cell) => cell.textContent)).toEqual(["Grão de café 1kg", "8", "10", "▼ -2"]);
    expect(within(region.getByRole("row", { name: /Filtro de papel/ })).getAllByRole("cell").map((cell) => cell.textContent)).toEqual(["Filtro de papel", "3", "3", "0"]);
    expect(region.getByRole("row", { name: /Açúcar/ })).toHaveTextContent("▲ +0,5");
  });

  it("applies a count only after it is confirmed, and says what it does", async () => {
    const fetchMock = mockApi(
      answer([open], (url, init) =>
        url.endsWith("/inventory-counts/i1/apply") && init?.method === "POST" ? { status: 200, body: { ...open, status: "applied" } } : undefined,
      ),
    );
    renderAt("/companies/c1/inventory-counts");
    await userEvent.click(await screen.findByRole("button", { name: "Aplicar o inventário 1" }));
    const region = within(screen.getByRole("region", { name: "Confirmação do inventário 1" }));
    expect(region.getByText(/um ajuste para cada produto/)).toBeInTheDocument();
    expect(calls(fetchMock, "POST")).toHaveLength(0);
    await userEvent.click(region.getByRole("button", { name: "Aplicar inventário" }));

    expect(await screen.findByText(/Inventário nº 1 aplicado: os ajustes foram lançados no estoque/)).toBeInTheDocument();
    expect(calls(fetchMock, "POST")).toHaveLength(1);
    expect(await screen.findByRole("row", { name: /Aplicado/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Aplicar o inventário 1" })).not.toBeInTheDocument();
  });

  it("shows the API's refusal when the count is not open any more", async () => {
    mockApi(
      answer([open], (url, init) =>
        url.endsWith("/inventory-counts/i1/apply") && init?.method === "POST"
          ? { status: 409, body: { error: "the inventory count is not open" } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/inventory-counts");
    await userEvent.click(await screen.findByRole("button", { name: "Aplicar o inventário 1" }));
    const region = within(screen.getByRole("region", { name: "Confirmação do inventário 1" }));
    await userEvent.click(region.getByRole("button", { name: "Aplicar inventário" }));
    expect(await region.findByRole("alert")).toHaveTextContent("the inventory count is not open");
  });

  it("changes an open count in the sheet, with its lines loaded", async () => {
    const fetchMock = mockApi(
      answer([open], (url, init) =>
        url.endsWith("/inventory-counts/i1") && init?.method === "PUT" ? { status: 200, body: { ...open, notes: "Revisto" } } : undefined,
      ),
    );
    renderAt("/companies/c1/inventory-counts");
    await userEvent.click(await screen.findByRole("button", { name: "Alterar o inventário 1" }));
    const sheet = within(screen.getByRole("heading", { name: "Alterar o inventário 1" }).closest("aside")!);
    expect(await sheet.findByLabelText("Quantidade contada do item 1")).toHaveValue("8");
    expect(sheet.getByLabelText("Produto do item 2")).toHaveValue("pr2");
    await userEvent.type(sheet.getByLabelText("Observações"), "Revisto");
    await userEvent.click(sheet.getByRole("button", { name: "Salvar alterações" }));

    expect(await screen.findByText("Inventário nº 1 atualizado.")).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "PUT")[0][1]?.body))).toEqual({
      counted_on: "2026-11-10",
      notes: "Revisto",
      lines: [
        { product_id: "pr1", counted: "8" },
        { product_id: "pr2", counted: "3" },
      ],
    });
  });

  it("invites the first inventory when there is none, with a button that opens the sheet", async () => {
    mockApi(answer([]));
    renderAt("/companies/c1/inventory-counts");

    expect(await screen.findByRole("heading", { name: "Realize um inventário para atualizar seu estoque" })).toBeInTheDocument();
    expect(screen.getByText(/ao custo médio, na data da contagem/)).toBeInTheDocument();
    // The button is in the empty state and in the page's actions: either opens the sheet.
    const buttons = screen.getAllByRole("button", { name: "Novo inventário" });
    await userEvent.click(buttons[buttons.length - 1]);
    expect(await screen.findByRole("heading", { name: "Novo inventário" })).toBeInTheDocument();
  });

  it("shows the empty invitation without a button to a role that only reads", async () => {
    mockApi(answer([], readOnly));
    renderAt("/companies/c1/inventory-counts");
    expect(await screen.findByRole("heading", { name: "Realize um inventário para atualizar seu estoque" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Novo inventário" })).not.toBeInTheDocument();
  });

  it("keeps the plain message when a search finds nothing", async () => {
    mockApi(answer([]));
    renderAt("/companies/c1/inventory-counts");
    await screen.findByRole("heading", { name: "Realize um inventário para atualizar seu estoque" });
    await userEvent.selectOptions(screen.getByLabelText("Situação"), "applied");
    expect(await screen.findByText("Nenhum inventário encontrado.")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Realize um inventário para atualizar seu estoque" })).not.toBeInTheDocument();
  });

  it("shows the list and nothing to write to a role that only reads", async () => {
    mockApi(answer([open], readOnly));
    renderAt("/companies/c1/inventory-counts");
    await screen.findByRole("row", { name: /10\/11\/2026/ });
    expect(screen.queryByRole("button", { name: "Novo inventário" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /(Alterar|Aplicar|Cancelar) o inventário/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Detalhes do inventário 1" })).toBeInTheDocument();
  });
});
