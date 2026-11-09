import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { InvoicePreview, Product, PurchaseOrder } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const product = (id: string, name: string, sku: string | null): Product => ({
  id,
  company_id: "c1",
  kind: "product",
  name,
  sku,
  unit: "KG",
  ncm: null,
  sale_price: null,
  cost_price: null,
  minimum_stock: null,
  active: true,
});
const beans = product("pr1", "Grão de café 1kg", "GRAO");
const filters = product("pr2", "Filtro de papel", "FILTRO");
const term = { id: "t1", company_id: "c1", name: "30 dias", installments: [{ days: 30, percent: "100" }], active: true };
const expense = { id: "cat1", company_id: "c1", parent_id: null, name: "Matéria-prima", dre_group: "cost_of_goods", cash_flow_group: "operating", level: 1, active: true };

const KEY = "35261112345678000195550010000001231000001230";

const invoice = (patch: Partial<InvoicePreview> = {}): InvoicePreview => ({
  access_key: KEY,
  number: "123",
  series: "1",
  issued_on: "2026-11-05",
  issuer: { document: "12345678000195", name: "Torrefação Serra Ltda" },
  total: "300",
  supplier_id: "p1",
  items: [
    { number: 1, code: "GRAO", ean: null, description: "Café torrado 1kg", ncm: null, unit: "KG", quantity: "10", unit_value: "25", product_value: "250", discount: "0", cost: "270.5", ipi: "20.5", icms_st: "0", icms: "0", product_id: "pr1" },
    { number: 2, code: "XYZ-9", ean: null, description: "Coador de pano", ncm: null, unit: "UN", quantity: "5", unit_value: "10", product_value: "50", discount: "0", cost: "29.5", ipi: "0", icms_st: "0", icms: "0", product_id: null },
  ],
  installments: [
    { due_date: "2026-12-05", amount: "150" },
    { due_date: "2027-01-05", amount: "150" },
  ],
  entered: null,
  ...patch,
});

const xml = () => new File(["<nfeProc/>"], "nota.xml", { type: "application/xml" });

const order = (patch: Partial<PurchaseOrder> = {}): PurchaseOrder => ({
  id: "o1",
  company_id: "c1",
  number: 7,
  supplier_id: "p1",
  ordered_on: "2026-11-05",
  expected_on: null,
  payment_term_id: null,
  category_id: "cat1",
  cost_center_id: null,
  notes: null,
  status: "received",
  invoice_key: null,
  lines: [{ product_id: "pr1", description: "Grão de café 1kg", quantity: "10", unit_price: "27.05", discount: "0", total: "270.5" }],
  total: "270.5",
  ...patch,
});

const calls = (fetchMock: ReturnType<typeof mockApi>, method: string) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === method);

const answer =
  (preview: InvoicePreview | Reply, extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined, orders: PurchaseOrder[] = []) =>
  (url: string, init?: RequestInit): Reply => {
    const special = extra(url, init);
    if (special) return special;
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/purchase-imports/preview")) return "status" in preview ? preview : { status: 200, body: preview };
    if (url.includes("/parties")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.includes("/products")) return { status: 200, body: { items: [beans, filters], next_cursor: null } };
    if (url.includes("/payment-terms")) return { status: 200, body: { items: [term], next_cursor: null } };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [expense] } };
    if (url.includes("/cost-centers")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.includes("/purchase-orders")) return { status: 200, body: { items: orders, next_cursor: null } };
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

/** Opens the sheet from the purchases page and reads the file, leaving the person on the preview. */
const readInvoice = async (file: File = xml()) => {
  await userEvent.click(await screen.findByRole("button", { name: "Novo" }));
  await userEvent.click(await screen.findByRole("menuitem", { name: "Entrar NF-e (XML)" }));
  const sheet = within((await screen.findByRole("heading", { name: "Entrada por XML (NF-e)" })).closest("aside")!);
  await userEvent.upload(await sheet.findByLabelText("Arquivo XML da NF-e"), file);
  await userEvent.click(sheet.getByRole("button", { name: "Ler arquivo" }));
  // What the invoice says is the first step after the file.
  await sheet.findByRole("heading", { name: "O que a nota diz" });
  return sheet;
};

type Sheet = Awaited<ReturnType<typeof readInvoice>>;

/** Goes on to the next step. */
const next = (sheet: Sheet) => userEvent.click(sheet.getByRole("button", { name: "Continuar" }));

/** Fills the nature of the purchase (the second step) and goes on to the products. */
const chooseNature = async (sheet: Sheet, extra?: (sheet: Sheet) => Promise<void> | void) => {
  await next(sheet);
  await userEvent.selectOptions(await sheet.findByLabelText("Categoria das contas a pagar"), "cat1");
  await extra?.(sheet);
  await next(sheet);
  await sheet.findByRole("heading", { name: "Produtos da NF-e" });
};

describe("purchase entry from an NF-e", () => {
  it("goes in order: the file, what the invoice says, the nature of the purchase, the products and a last look", async () => {
    const fetchMock = mockApi(answer(invoice()));
    renderAt("/companies/c1/purchases");
    const file = xml();
    const sheet = await readInvoice(file);

    // The five steps are shown, and the one the person is on is marked.
    const steps = within(sheet.getByRole("list", { name: "Etapas" }));
    // The file is done (a tick in place of its number), the rest still carry theirs.
    expect(steps.getAllByRole("listitem").map((item) => item.textContent)).toEqual([
      "Arquivo XML",
      "2Dados da nota",
      "3Natureza da compra",
      "4Produtos",
      "5Concluir",
    ]);
    expect(steps.getByText("Dados da nota").closest("li")).toHaveAttribute("aria-current", "step");

    // Step 2: what the invoice says.
    expect(sheet.getByText("Torrefação Serra Ltda").closest("p")).toHaveTextContent("12345678000195");
    expect(sheet.getByText(/NF-e nº 123, série 1/)).toHaveTextContent("R$ 300,00");
    expect(sheet.getByText(/NF-e nº 123, série 1/)).toHaveTextContent("05/11/2026");
    const parcels = within(sheet.getByRole("list", { name: "Parcelas da nota" }));
    expect(parcels.getByText("05/12/2026 · R$ 150,00")).toBeInTheDocument();
    expect(parcels.getByText("05/01/2027 · R$ 150,00")).toBeInTheDocument();
    await next(sheet);

    // Step 3: the nature; the invoice gives installments, so the payment term does not apply, and a category is needed.
    expect(await sheet.findByRole("heading", { name: "Natureza da compra" })).toBeInTheDocument();
    expect(sheet.queryByLabelText("Prazo de pagamento")).not.toBeInTheDocument();
    expect(sheet.getByRole("button", { name: "Continuar" })).toBeDisabled();
    expect(sheet.getByText("Escolha a categoria das contas a pagar.")).toBeInTheDocument();
    await userEvent.selectOptions(sheet.getByLabelText("Categoria das contas a pagar"), "cat1");
    await next(sheet);

    // Step 4: each item with its product; one has none, and that stops the way on.
    const first = await sheet.findByRole("row", { name: /GRAO/ });
    expect(first).toHaveTextContent("10 KG");
    expect(first).toHaveTextContent("R$ 270,50");
    expect(first).toHaveTextContent("R$ 20,50");
    expect(within(first).getByLabelText("Produto do item 1")).toHaveValue("pr1");
    const second = sheet.getByRole("row", { name: /XYZ-9/ });
    expect(within(second).getByLabelText("Produto do item 2")).toHaveValue("");
    expect(within(second).getByText("Sem produto")).toBeInTheDocument();
    expect(sheet.getByRole("button", { name: "Continuar" })).toBeDisabled();

    // The XML went to the preview as it is, as the body, and only once.
    const [, init] = calls(fetchMock, "POST")[0];
    expect(init?.body).toBe(file);
    expect(new Headers(init?.headers).get("Content-Type")).toBe("application/xml");
    expect(calls(fetchMock, "POST")).toHaveLength(1);
  });

  it("goes back a step without losing what was chosen", async () => {
    mockApi(answer(invoice()));
    renderAt("/companies/c1/purchases");
    const sheet = await readInvoice();
    await chooseNature(sheet);

    await userEvent.click(sheet.getByRole("button", { name: "Voltar" }));
    expect(await sheet.findByLabelText("Categoria das contas a pagar")).toHaveValue("cat1");
    await userEvent.click(sheet.getByRole("button", { name: "Voltar" }));
    expect(await sheet.findByRole("heading", { name: "O que a nota diz" })).toBeInTheDocument();
  });

  it("enters the purchase with the chosen products, category and day, the file as the body, after a last look", async () => {
    const fetchMock = mockApi(
      answer(invoice(), (url, init) =>
        url.includes("/purchase-imports?") && init?.method === "POST"
          ? { status: 201, body: { purchase_id: "o1", number: 7, status: "received", receipt: {}, already_entered: false } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/purchases");
    const file = xml();
    const sheet = await readInvoice(file);

    await chooseNature(sheet, () => {
      fireEvent.change(sheet.getByLabelText("Dia em que a mercadoria chegou"), { target: { value: "2026-11-08" } });
    });
    await userEvent.selectOptions(sheet.getByLabelText("Produto do item 2"), "pr2");
    await next(sheet);

    // The last look says what will be made, and nothing is sent until it is confirmed.
    expect(await sheet.findByRole("heading", { name: "Concluir a entrada" })).toBeInTheDocument();
    expect(sheet.getByText("Torrefação Serra Ltda")).toBeInTheDocument();
    expect(sheet.getByText("2 parcelas, as da nota")).toBeInTheDocument();
    expect(sheet.getByText("08/11/2026")).toBeInTheDocument();
    expect(calls(fetchMock, "POST")).toHaveLength(1);
    await userEvent.click(sheet.getByRole("button", { name: "Entrar compra" }));

    expect(await screen.findByText("Compra nº 7 criada a partir da NF-e.")).toBeInTheDocument();
    const [entryUrl, init] = calls(fetchMock, "POST")[1];
    const url = new URL(String(entryUrl), "http://localhost");
    expect(url.pathname).toBe("/api/companies/c1/purchase-imports");
    expect(Object.fromEntries(url.searchParams)).toEqual({ category_id: "cat1", received_on: "2026-11-08", products: "pr1,pr2" });
    expect(init?.body).toBe(file);
  });

  it("asks for the payment term only when the invoice gives no installments, and sends it", async () => {
    const fetchMock = mockApi(
      answer(invoice({ installments: [], items: invoice().items.map((item) => ({ ...item, product_id: "pr1" })) }), (url, init) =>
        url.includes("/purchase-imports?") && init?.method === "POST"
          ? { status: 201, body: { purchase_id: "o1", number: 8, status: "received", receipt: {}, already_entered: false } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/purchases");
    const sheet = await readInvoice();

    expect(sheet.getByText("A nota não informa parcelas.")).toBeInTheDocument();
    await chooseNature(sheet, async () => {
      await userEvent.selectOptions(sheet.getByLabelText("Prazo de pagamento"), "t1");
    });
    await next(sheet);
    expect(await sheet.findByText("30 dias")).toBeInTheDocument();
    await userEvent.click(sheet.getByRole("button", { name: "Entrar compra" }));

    expect(await screen.findByText("Compra nº 8 criada a partir da NF-e.")).toBeInTheDocument();
    const url = new URL(String(calls(fetchMock, "POST")[1][0]), "http://localhost");
    expect(url.searchParams.get("payment_term_id")).toBe("t1");
  });

  it("does not go on with a day that is not a whole date, like a year typed half way", async () => {
    mockApi(answer(invoice()));
    renderAt("/companies/c1/purchases");
    const sheet = await readInvoice();
    await next(sheet);
    await userEvent.selectOptions(await sheet.findByLabelText("Categoria das contas a pagar"), "cat1");
    fireEvent.change(sheet.getByLabelText("Dia em que a mercadoria chegou"), { target: { value: "0002-11-06" } });
    expect(sheet.getByText("Informe o dia em que a mercadoria chegou.")).toBeInTheDocument();
    expect(sheet.getByRole("button", { name: "Continuar" })).toBeDisabled();
  });

  it("shows the API's reason on the last step when the entry fails, and stays there", async () => {
    mockApi(
      answer(invoice({ items: invoice().items.map((item) => ({ ...item, product_id: "pr1" })) }), (url, init) =>
        url.includes("/purchase-imports?") && init?.method === "POST" ? { status: 409, body: { error: "the invoice changed meanwhile" } } : undefined,
      ),
    );
    renderAt("/companies/c1/purchases");
    const sheet = await readInvoice();
    await chooseNature(sheet);
    await next(sheet);
    await userEvent.click(await sheet.findByRole("button", { name: "Entrar compra" }));

    expect(await sheet.findByRole("alert")).toHaveTextContent("the invoice changed meanwhile");
    expect(sheet.getByRole("heading", { name: "Concluir a entrada" })).toBeInTheDocument();
  });

  it("puts away the API's reason once the person goes back and changes something, and shows a new one for a new attempt", async () => {
    mockApi(
      answer(invoice({ items: invoice().items.map((item) => ({ ...item, product_id: "pr1" })) }), (url, init) =>
        url.includes("/purchase-imports?") && init?.method === "POST" ? { status: 409, body: { error: "the invoice changed meanwhile" } } : undefined,
      ),
    );
    renderAt("/companies/c1/purchases");
    const sheet = await readInvoice();
    await chooseNature(sheet);
    await next(sheet);
    await userEvent.click(await sheet.findByRole("button", { name: "Entrar compra" }));
    expect(await sheet.findByRole("alert")).toHaveTextContent("the invoice changed meanwhile");

    // Back to the nature, change the category, forward again: the old reason is not shown.
    await userEvent.click(sheet.getByRole("button", { name: "Voltar" }));
    await userEvent.click(sheet.getByRole("button", { name: "Voltar" }));
    await userEvent.selectOptions(await sheet.findByLabelText("Categoria das contas a pagar"), "cat1");
    await next(sheet);
    await next(sheet);
    await sheet.findByRole("heading", { name: "Concluir a entrada" });
    expect(sheet.queryByRole("alert")).not.toBeInTheDocument();

    // A new attempt that fails the same way shows it again.
    await userEvent.click(sheet.getByRole("button", { name: "Entrar compra" }));
    expect(await sheet.findByRole("alert")).toHaveTextContent("the invoice changed meanwhile");
  });

  it("does not let an invoice from an unregistered supplier be entered", async () => {
    mockApi(answer(invoice({ supplier_id: null, items: invoice().items.map((item) => ({ ...item, product_id: "pr1" })) })));
    renderAt("/companies/c1/purchases");
    const sheet = await readInvoice();

    expect(sheet.getByText(/Fornecedor não cadastrado.*12345678000195/)).toBeInTheDocument();
    expect(sheet.getByRole("button", { name: "Continuar" })).toBeDisabled();
  });

  it("says when the invoice was already entered, with a link to the purchases, and does not go on", async () => {
    mockApi(
      answer(invoice({ entered: { purchase_id: "o1", number: 7, status: "received" }, items: invoice().items.map((item) => ({ ...item, product_id: "pr1" })) })),
    );
    renderAt("/companies/c1/purchases");
    const sheet = await readInvoice();

    expect(sheet.getByText("Esta nota já foi entrada como a compra nº 7.")).toBeInTheDocument();
    expect(sheet.getByRole("link", { name: "Ver nas compras" })).toHaveAttribute("href", "/companies/c1/purchases");
    expect(sheet.getByRole("button", { name: "Continuar" })).toBeDisabled();
  });

  it("shows the API's message for a file it refuses", async () => {
    mockApi(answer({ status: 422, body: { error: "not a valid XML" } }));
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Novo" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Entrar NF-e (XML)" }));
    const sheet = within((await screen.findByRole("heading", { name: "Entrada por XML (NF-e)" })).closest("aside")!);
    await userEvent.upload(await sheet.findByLabelText("Arquivo XML da NF-e"), xml());
    await userEvent.click(sheet.getByRole("button", { name: "Ler arquivo" }));

    expect(await sheet.findByRole("alert")).toHaveTextContent("not a valid XML");
    // Still on the first step: another file can be chosen.
    expect(sheet.getByRole("button", { name: "Ler arquivo" })).toBeInTheDocument();
  });

  it("refuses a file that is not an XML or is too big before sending it", async () => {
    const fetchMock = mockApi(answer(invoice()));
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Novo" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Entrar NF-e (XML)" }));
    const sheet = within((await screen.findByRole("heading", { name: "Entrada por XML (NF-e)" })).closest("aside")!);
    const input = await sheet.findByLabelText("Arquivo XML da NF-e");

    fireEvent.change(input, { target: { files: [new File(["x"], "nota.pdf", { type: "application/pdf" })] } });
    expect(await sheet.findByRole("alert")).toHaveTextContent("Escolha o arquivo XML da nota");
    expect(sheet.getByRole("button", { name: "Ler arquivo" })).toBeDisabled();
    fireEvent.change(input, { target: { files: [new File([new Uint8Array(1024 * 1024 + 1)], "grande.xml", { type: "application/xml" })] } });
    expect(await sheet.findByRole("alert")).toHaveTextContent("mais de 1 MiB");
    expect(sheet.getByRole("button", { name: "Ler arquivo" })).toBeDisabled();
    expect(calls(fetchMock, "POST")).toHaveLength(0);
  });

  it("takes a file dropped on the box like a chosen one, and refuses a dropped file that is not an XML", async () => {
    const fetchMock = mockApi(answer(invoice()));
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Novo" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Entrar NF-e (XML)" }));
    const sheet = within((await screen.findByRole("heading", { name: "Entrada por XML (NF-e)" })).closest("aside")!);
    await userEvent.click(sheet.getByRole("button", { name: "Sobre Arquivo da nota" }));
    expect(sheet.getByRole("tooltip")).toHaveTextContent(/A NF-e fica salva no seu computador/);
    expect(sheet.getByText("Ou arraste o XML para este espaço")).toBeInTheDocument();
    const zone = sheet.getByTestId("invoice-dropzone");

    fireEvent.dragOver(zone);
    expect(zone).toHaveClass("dropzone-active");
    fireEvent.dragLeave(zone);
    expect(zone).not.toHaveClass("dropzone-active");

    fireEvent.drop(zone, { dataTransfer: { files: [new File(["x"], "notas.txt", { type: "text/plain" })] } });
    expect(await sheet.findByRole("alert")).toHaveTextContent("Escolha o arquivo XML da nota");
    expect(sheet.getByRole("button", { name: "Ler arquivo" })).toBeDisabled();
    expect(calls(fetchMock, "POST")).toHaveLength(0);

    const file = xml();
    fireEvent.drop(zone, { dataTransfer: { files: [file] } });
    expect(await sheet.findByText("nota.xml")).toBeInTheDocument();
    await userEvent.click(sheet.getByRole("button", { name: "Ler arquivo" }));
    await sheet.findByRole("heading", { name: "O que a nota diz" });
    expect(calls(fetchMock, "POST")[0][1]?.body).toBe(file);
  });

  it("offers no entry from an NF-e to a role that only reads", async () => {
    mockApi(answer(invoice(), readOnly));
    renderAt("/companies/c1/purchases");
    await screen.findByRole("heading", { name: "Compras" });
    expect(screen.queryByRole("button", { name: "Novo" })).not.toBeInTheDocument();
  });

  it("shows the access key of an order that came from an NF-e in its details", async () => {
    mockApi(answer(invoice(), (url) => (url.endsWith("/receipts") ? { status: 200, body: { items: [] } } : undefined), [order({ invoice_key: KEY })]));
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Detalhes da compra 7" }));
    expect(await screen.findByText(`NF-e (chave): ${KEY}`)).toBeInTheDocument();
  });
});
