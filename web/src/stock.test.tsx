import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Product, StockAlert, StockMovement, StockPosition } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const beans: Product = {
  id: "pr1",
  company_id: "c1",
  kind: "product",
  name: "Grão de café 1kg",
  sku: "grao",
  unit: "KG",
  ncm: null,
  sale_price: "40",
  cost_price: null,
  minimum_stock: "5",
  active: true,
};

const position = (patch: Partial<StockPosition>): StockPosition => ({
  product_id: "pr1",
  name: "Grão de café 1kg",
  sku: "grao",
  unit: "KG",
  active: true,
  minimum_stock: "5",
  quantity: "20",
  value: "120",
  average_cost: "6.000000",
  ...patch,
});

const movement = (patch: Partial<StockMovement>): StockMovement => ({
  id: "m1",
  product_id: "pr1",
  kind: "in",
  quantity: "10",
  value: "50",
  occurred_on: "2026-11-10",
  origin: null,
  reverses: null,
  note: null,
  ...patch,
});

const alert = (patch: Partial<StockAlert>): StockAlert => ({
  product_id: "pr2",
  name: "Filtro de papel",
  sku: "filtro",
  unit: "UN",
  quantity: "3",
  minimum_stock: "10",
  shortfall: "7",
  ...patch,
});

const calls = (fetchMock: ReturnType<typeof mockApi>, method: string) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === method);

const summaryOf = (positions: StockPosition[]) => ({
  total: positions.length,
  in_stock: positions.filter((p) => Number(p.quantity) > 0).length,
  below_minimum: positions.filter((p) => p.minimum_stock !== null && Number(p.quantity) < Number(p.minimum_stock)).length,
  depleted: positions.filter((p) => Number(p.quantity) <= 0).length,
  value: positions.reduce((sum, p) => sum + Number(p.value), 0).toString(),
});

const answer =
  (extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined, positions: StockPosition[] = [position({})], movements: StockMovement[] = [], alerts: StockAlert[] = []) =>
  (url: string, init?: RequestInit): Reply => {
    const special = extra(url, init);
    if (special) return special;
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/stock/alerts")) return { status: 200, body: { items: alerts } };
    if (url.includes("/stock/positions/summary")) return { status: 200, body: summaryOf(positions) };
    if (url.includes("/stock/positions")) return { status: 200, body: { items: positions, next_cursor: null } };
    if (url.includes("/stock/movements/kind-counts")) {
      const of = (kind: StockMovement["kind"]) => movements.filter((movement) => movement.kind === kind).length;
      return { status: 200, body: { in: of("in"), out: of("out"), adjustment: of("adjustment") } };
    }
    if (url.includes("/stock/movements")) return { status: 200, body: { items: movements, next_cursor: null } };
    if (url.includes("/products")) return { status: 200, body: { items: [beans], next_cursor: null } };
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
  await userEvent.click(await screen.findByRole("button", { name: "Novo movimento" }));
  const sheet = within((await screen.findByRole("heading", { name: "Novo movimento de estoque" })).closest("aside")!);
  await sheet.findByLabelText("Produto");
  return sheet;
};

const stubDownloads = () => {
  const saved: { name: string; text: string }[] = [];
  vi.stubGlobal("URL", {
    createObjectURL: (blob: Blob) => {
      saved.push({ name: "", text: "" });
      void blob.text().then((text) => (saved[saved.length - 1].text = text));
      return "blob:x";
    },
    revokeObjectURL: () => undefined,
  });
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) {
    saved[saved.length - 1].name = this.download;
  });
  return saved;
};

describe("stock positions", () => {
  it("shows what is on hand with its unit, average cost and value, and flags what the server says is low, or negative", async () => {
    mockApi(
      answer(undefined, [
        position({}),
        position({ product_id: "pr2", name: "Filtro de papel", sku: null, unit: "UN", quantity: "3", value: "9", average_cost: "3.333333", minimum_stock: "10" }),
        position({ product_id: "pr3", name: "Açúcar", sku: null, quantity: "-2", value: "0", average_cost: null, minimum_stock: null }),
      ], [], [alert({})]),
    );
    renderAt("/companies/c1/stock");

    const row = await screen.findByRole("row", { name: /Grão de café 1kg/ });
    expect(row).toHaveTextContent("20 KG");
    expect(row).toHaveTextContent("R$ 6,00");
    expect(row).toHaveTextContent("R$ 120,00");
    expect(within(row).queryByText("Abaixo do mínimo")).not.toBeInTheDocument();
    const low = screen.getByRole("row", { name: /Filtro de papel/ });
    expect(low).toHaveTextContent("3 UN");
    expect(low).toHaveTextContent("R$ 3,333333");
    expect(within(low).getByText("Abaixo do mínimo")).toBeInTheDocument();
    const negative = screen.getByRole("row", { name: /Açúcar/ });
    expect(within(negative).getByText("Saldo negativo")).toBeInTheDocument();
    expect(negative).toHaveTextContent("—");
  });

  it("counts the products by situation in tiles that filter the list, with the total cost under it", async () => {
    const positions = [
      position({}),
      position({ product_id: "pr2", name: "Filtro de papel", sku: null, unit: "UN", quantity: "3", value: "9", average_cost: "3.333333", minimum_stock: "10" }),
      position({ product_id: "pr3", name: "Açúcar", sku: null, quantity: "0", value: "0", average_cost: null, minimum_stock: null }),
    ];
    const fetchMock = mockApi(answer(undefined, positions));
    renderAt("/companies/c1/stock");

    const tiles = within(await screen.findByRole("group", { name: "Situação" }));
    await vi.waitFor(() => expect(tiles.getByRole("button", { name: /Todos/ })).toHaveTextContent("3"));
    expect(tiles.getByRole("button", { name: /Em estoque/ })).toHaveTextContent("2");
    expect(tiles.getByRole("button", { name: /Abaixo do mínimo/ })).toHaveTextContent("1");
    expect(tiles.getByRole("button", { name: /Esgotado/ })).toHaveTextContent("1");
    expect(await screen.findByText(/Total: 3 produtos · Custo total R\$ 129,00/)).toBeInTheDocument();

    await userEvent.click(tiles.getByRole("button", { name: /Esgotado/ }));
    expect(tiles.getByRole("button", { name: /Esgotado/ })).toHaveAttribute("aria-pressed", "true");
    await vi.waitFor(() => expect(fetchMock.mock.calls.some(([url]) => String(url).includes("/stock/positions?situation=depleted"))).toBe(true));
    // Every tile keeps its number: the summary does not take the situation.
    expect(fetchMock.mock.calls.some(([url]) => String(url).includes("/summary") && String(url).includes("situation"))).toBe(false);
  });

  it("exports the position with plain decimals", async () => {
    const saved = stubDownloads();
    mockApi(answer());
    renderAt("/companies/c1/stock");
    await screen.findByRole("row", { name: /Grão de café 1kg/ });
    await userEvent.click(screen.getByRole("button", { name: "Exportar" }));
    expect(await screen.findByText("1 linha exportada.")).toBeInTheDocument();
    await vi.waitFor(() => expect(saved[0].text).not.toBe(""));
    expect(saved[0].name).toBe("posicao-do-estoque.csv");
    expect(saved[0].text).toContain("Produto;SKU;Unidade;Quantidade;Custo médio;Valor\r\n");
    expect(saved[0].text).toContain("Grão de café 1kg;");
  });

  it("asks for the balance on the day chosen", async () => {
    const fetchMock = mockApi(answer());
    renderAt("/companies/c1/stock");
    await screen.findByRole("row", { name: /Grão de café 1kg/ });
    fireEvent.change(screen.getByLabelText("Em"), { target: { value: "2026-10-31" } });
    await vi.waitFor(() => expect(fetchMock.mock.calls.some(([url]) => String(url).includes("/stock/positions?on=2026-10-31"))).toBe(true));
  });
});

describe("new stock movement", () => {
  it("records an entry with a unit cost", async () => {
    const fetchMock = mockApi(
      answer((url, init) => (url.endsWith("/stock/movements") && init?.method === "POST" ? { status: 201, body: movement({}) } : undefined)),
    );
    renderAt("/companies/c1/stock");
    const sheet = await openSheet();

    await userEvent.selectOptions(sheet.getByLabelText("Produto"), "pr1");
    await userEvent.type(sheet.getByLabelText("Quantidade"), "10");
    await userEvent.click(sheet.getByRole("radio", { name: "Custo unitário" }));
    await userEvent.type(sheet.getByLabelText("Valor do custo unitário"), "5,5");
    fireEvent.change(sheet.getByLabelText("Data"), { target: { value: "2026-11-10" } });
    await userEvent.click(sheet.getByRole("button", { name: "Registrar movimento" }));

    expect(await screen.findByText("Entrada de estoque registrada.")).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({
      product_id: "pr1",
      kind: "in",
      quantity: "10",
      unit_cost: "5.5",
      occurred_on: "2026-11-10",
    });
  });

  it("records an exit without asking for a cost", async () => {
    const fetchMock = mockApi(
      answer((url, init) => (url.endsWith("/stock/movements") && init?.method === "POST" ? { status: 201, body: movement({ kind: "out", quantity: "-2", value: "-12" }) } : undefined)),
    );
    renderAt("/companies/c1/stock");
    const sheet = await openSheet();

    await userEvent.click(sheet.getByRole("radio", { name: "Saída" }));
    expect(sheet.queryByLabelText("Valor do custo total")).not.toBeInTheDocument();
    await userEvent.selectOptions(sheet.getByLabelText("Produto"), "pr1");
    await userEvent.type(sheet.getByLabelText("Quantidade"), "2");
    fireEvent.change(sheet.getByLabelText("Data"), { target: { value: "2026-11-10" } });
    await userEvent.type(sheet.getByLabelText("Observação"), "Quebra");
    await userEvent.click(sheet.getByRole("button", { name: "Registrar movimento" }));

    expect(await screen.findByText("Saída de estoque registrada.")).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({
      product_id: "pr1",
      kind: "out",
      quantity: "2",
      occurred_on: "2026-11-10",
      note: "Quebra",
    });
  });

  it("refuses a quantity or a unit cost that could be thousands, and takes a negative adjustment with a comma", async () => {
    const fetchMock = mockApi(
      answer((url, init) => (url.endsWith("/stock/movements") && init?.method === "POST" ? { status: 201, body: movement({ kind: "adjustment", quantity: "-1.5", value: "-9" }) } : undefined)),
    );
    renderAt("/companies/c1/stock");
    const sheet = await openSheet();
    await userEvent.selectOptions(sheet.getByLabelText("Produto"), "pr1");
    await userEvent.type(sheet.getByLabelText("Quantidade"), "1.500");
    await userEvent.type(sheet.getByLabelText("Valor do custo total"), "50");
    await userEvent.click(sheet.getByRole("button", { name: "Registrar movimento" }));
    expect(await sheet.findByRole("alert")).toHaveTextContent("1.500 pode ser mil e quinhentos ou um e meio");

    await userEvent.clear(sheet.getByLabelText("Quantidade"));
    await userEvent.type(sheet.getByLabelText("Quantidade"), "2");
    await userEvent.clear(sheet.getByLabelText("Valor do custo total"));
    await userEvent.type(sheet.getByLabelText("Valor do custo total"), "1.234");
    await userEvent.click(sheet.getByRole("button", { name: "Registrar movimento" }));
    expect(await sheet.findByRole("alert")).toHaveTextContent("1.500 pode ser mil e quinhentos");
    expect(calls(fetchMock, "POST")).toHaveLength(0);

    await userEvent.click(sheet.getByRole("radio", { name: "Ajuste" }));
    await userEvent.clear(sheet.getByLabelText("Quantidade"));
    await userEvent.type(sheet.getByLabelText("Quantidade"), "-1,5");
    fireEvent.change(sheet.getByLabelText("Data"), { target: { value: "2026-11-10" } });
    await userEvent.click(sheet.getByRole("button", { name: "Registrar movimento" }));
    await vi.waitFor(() => expect(calls(fetchMock, "POST")).toHaveLength(1));
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({
      product_id: "pr1",
      kind: "adjustment",
      quantity: "-1.5",
      occurred_on: "2026-11-10",
    });
  });

  it("refuses an entry with no cost before asking the API, and shows the API's refusal of an exit beyond the balance", async () => {
    const fetchMock = mockApi(
      answer((url, init) =>
        url.endsWith("/stock/movements") && init?.method === "POST"
          ? { status: 409, body: { error: "not enough stock of Grão de café 1kg: 3 on hand" } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/stock");
    const sheet = await openSheet();
    await userEvent.selectOptions(sheet.getByLabelText("Produto"), "pr1");
    await userEvent.type(sheet.getByLabelText("Quantidade"), "10");
    await userEvent.click(sheet.getByRole("button", { name: "Registrar movimento" }));
    expect(await sheet.findByRole("alert")).toHaveTextContent("Informe o custo total da entrada");
    expect(calls(fetchMock, "POST")).toHaveLength(0);

    await userEvent.click(sheet.getByRole("radio", { name: "Saída" }));
    await userEvent.click(sheet.getByRole("button", { name: "Registrar movimento" }));
    expect(await sheet.findByRole("alert")).toHaveTextContent("not enough stock of Grão de café 1kg: 3 on hand");
  });
});

describe("stock movements", () => {
  const ledger = [
    movement({ id: "m1", kind: "in", origin: { kind: "purchase_receipt", id: "r1", line: 1 } }),
    movement({ id: "m2", kind: "out", quantity: "-2", value: "-12", origin: { kind: "sale", id: "s1", line: 1 }, occurred_on: "2026-11-09" }),
    movement({ id: "m3", kind: "adjustment", quantity: "-1", value: "-6", occurred_on: "2026-11-08", note: "Contagem" }),
    movement({ id: "m4", kind: "in", quantity: "1", value: "6", occurred_on: "2026-11-08", origin: { kind: "sale_return", id: "t1", line: 1 } }),
  ];

  it("shows each movement with its signed quantity, value and origin", async () => {
    mockApi(answer(undefined, [], ledger));
    renderAt("/companies/c1/stock");
    await userEvent.click(await screen.findByRole("button", { name: "Movimentações" }));

    const purchase = await screen.findByRole("row", { name: /Compra/ });
    expect(purchase).toHaveTextContent("10/11/2026");
    expect(purchase).toHaveTextContent("Entrada");
    expect(purchase).toHaveTextContent("+10");
    expect(purchase).toHaveTextContent("R$ 50,00");
    const sale = screen.getByRole("row", { name: /Venda/ });
    expect(sale).toHaveTextContent("Saída");
    expect(sale).toHaveTextContent("-2");
    expect(sale).toHaveTextContent("R$ -12,00");
    expect(screen.getByRole("row", { name: /Devolução/ })).toHaveTextContent("+1");
    const manual = screen.getByRole("row", { name: /Manual/ });
    expect(manual).toHaveTextContent("Ajuste");
    expect(manual).toHaveTextContent("Contagem");
    // Only what was written by hand can be undone.
    expect(screen.getAllByRole("button", { name: /Desfazer o movimento/ })).toHaveLength(1);
  });

  it("undoes a manual movement on the day chosen, and shows what undoes another", async () => {
    const fetchMock = mockApi(
      answer(
        (url, init) =>
          url.endsWith("/stock/movements/m3/reverse") && init?.method === "POST"
            ? { status: 201, body: movement({ id: "m5", kind: "adjustment", quantity: "1", value: "6", reverses: "m3" }) }
            : undefined,
        [],
        [...ledger, movement({ id: "m6", kind: "in", quantity: "4", value: "20", occurred_on: "2026-11-07" }), movement({ id: "m7", kind: "out", quantity: "-4", value: "-20", occurred_on: "2026-11-11", reverses: "m6" })],
      ),
    );
    renderAt("/companies/c1/stock");
    await userEvent.click(await screen.findByRole("button", { name: "Movimentações" }));

    const reversal = await screen.findByRole("row", { name: /Desfaz movimento/ });
    expect(reversal).toHaveTextContent("-4");
    expect(screen.getByRole("row", { name: /Desfeito/ })).toHaveTextContent("07/11/2026");
    expect(screen.getAllByRole("button", { name: /Desfazer o movimento/ })).toHaveLength(1);

    await userEvent.click(screen.getByRole("button", { name: /Desfazer o movimento de 08\/11\/2026/ }));
    const region = within(screen.getByRole("region", { name: /Desfazer o movimento de 08\/11\/2026/ }));
    fireEvent.change(region.getByLabelText("Dia do desfazimento"), { target: { value: "2026-11-12" } });
    await userEvent.click(region.getByRole("button", { name: "Desfazer movimento" }));

    expect(await screen.findByText(/Movimento desfeito/)).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "POST")[0][1]?.body))).toEqual({ occurred_on: "2026-11-12" });
  });

  it("counts the ledger by kind in tiles that filter it", async () => {
    const fetchMock = mockApi(answer(undefined, [], ledger));
    renderAt("/companies/c1/stock");
    await userEvent.click(await screen.findByRole("button", { name: "Movimentações" }));
    await screen.findByRole("row", { name: /Compra/ });
    const tiles = within(screen.getByRole("group", { name: "Tipo" }));
    const kinds = (kind: string) => ledger.filter((entry) => entry.kind === kind).length;
    await vi.waitFor(() => expect(tiles.getByRole("button", { name: /Todas/ })).toHaveTextContent(String(ledger.length)));
    expect(tiles.getByRole("button", { name: /Entradas/ })).toHaveTextContent(String(kinds("in")));
    expect(tiles.getByRole("button", { name: /Saídas/ })).toHaveTextContent(String(kinds("out")));

    await userEvent.click(tiles.getByRole("button", { name: /Saídas/ }));
    await vi.waitFor(() => expect(fetchMock.mock.calls.some(([url]) => String(url).includes("/stock/movements?kind=out"))).toBe(true));
    // The counts follow the product and days, never the kind.
    expect(fetchMock.mock.calls.some(([url]) => String(url).includes("kind-counts?kind"))).toBe(false);
  });
});

describe("stock for a role that only reads", () => {
  it("offers no new movement and no undoing", async () => {
    mockApi(answer(readOnly, [position({})], [movement({})]));
    renderAt("/companies/c1/stock");
    await screen.findByRole("row", { name: /Grão de café 1kg/ });
    expect(screen.queryByRole("button", { name: "Novo movimento" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Movimentações" }));
    await screen.findByRole("row", { name: /Manual/ });
    expect(screen.queryByRole("button", { name: /Desfazer/ })).not.toBeInTheDocument();
  });
});

describe("stock alerts", () => {
  it("counts the products below their minimum on the tab and lists them with what is missing", async () => {
    mockApi(answer(undefined, [], [], [alert({}), alert({ product_id: "pr1", name: "Grão de café 1kg", sku: null, unit: "KG", quantity: "0.5", minimum_stock: "5", shortfall: "4.5" })]));
    renderAt("/companies/c1/stock");

    await userEvent.click(await screen.findByRole("button", { name: "Abaixo do mínimo (2)" }));
    const filter = await screen.findByRole("row", { name: /Filtro de papel/ });
    expect(filter).toHaveTextContent("filtro");
    expect(filter).toHaveTextContent("3 UN");
    expect(filter).toHaveTextContent("10 UN");
    expect(filter).toHaveTextContent("7 UN");
    expect(screen.getByRole("row", { name: /Grão de café 1kg/ })).toHaveTextContent("4,5 KG");
  });

  it("says so when nothing is below the minimum", async () => {
    mockApi(answer());
    renderAt("/companies/c1/stock");
    await userEvent.click(await screen.findByRole("button", { name: "Abaixo do mínimo" }));
    expect(await screen.findByText("Nenhum produto abaixo do mínimo.")).toBeInTheDocument();
  });
});

describe("stock settings", () => {
  const history = [
    { at: 1_700_000_000_000, actor: { kind: "user", id: "u1", name: "Ana" }, action: "updated", changes: [{ field: "block_sale_without_balance", from: "false", to: "true" }] },
  ];
  const withSettings =
    (stored: boolean, extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined) =>
    (url: string, init?: RequestInit): Reply | undefined => {
      if (url.endsWith("/stock/settings/history")) return { status: 200, body: history };
      if (url.endsWith("/stock/settings") && init?.method === "PUT") return { status: 200, body: JSON.parse(String(init.body)) };
      if (url.endsWith("/stock/settings")) return { status: 200, body: { block_sale_without_balance: stored } };
      return extra(url, init);
    };

  it("turns the block on with the whole setting in the body, and shows its trail", async () => {
    const fetchMock = mockApi(answer(withSettings(false)));
    renderAt("/companies/c1/stock");
    await userEvent.click(within(await screen.findByRole("group", { name: "Visão do estoque" })).getByRole("button", { name: "Configurações" }));

    const toggle = await screen.findByRole("switch", { name: "Bloquear venda sem saldo" });
    expect(toggle).not.toBeChecked();
    expect(screen.getByText(/concluir uma venda que o estoque não cobre é recusado/)).toBeInTheDocument();
    expect(await screen.findByText(/Desligado → Ligado/)).toBeInTheDocument();
    await userEvent.click(toggle);

    expect(await screen.findByText("Configuração salva.")).toBeInTheDocument();
    expect(JSON.parse(String(calls(fetchMock, "PUT")[0][1]?.body))).toEqual({ block_sale_without_balance: true });
    expect(screen.getByRole("switch", { name: "Bloquear venda sem saldo" })).toBeChecked();
  });

  it("shows the switch to a role that only reads, disabled", async () => {
    mockApi(answer((url, init) => readOnly(url) ?? withSettings(true)(url, init)));
    renderAt("/companies/c1/stock");
    await userEvent.click(within(await screen.findByRole("group", { name: "Visão do estoque" })).getByRole("button", { name: "Configurações" }));
    const toggle = await screen.findByRole("switch", { name: "Bloquear venda sem saldo" });
    expect(toggle).toBeChecked();
    expect(toggle).toBeDisabled();
  });
});
