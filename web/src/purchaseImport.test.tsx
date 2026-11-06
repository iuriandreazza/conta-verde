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
  await userEvent.click(await screen.findByRole("button", { name: "Entrar NF-e (XML)" }));
  const sheet = within((await screen.findByRole("heading", { name: "Entrada por XML (NF-e)" })).closest("aside")!);
  await userEvent.upload(await sheet.findByLabelText("Arquivo XML da NF-e"), file);
  await userEvent.click(sheet.getByRole("button", { name: "Ler arquivo" }));
  await sheet.findByRole("button", { name: "Entrar compra" });
  return sheet;
};

describe("purchase entry from an NF-e", () => {
  it("shows what the invoice says: the issuer, the installments, and each item with its product", async () => {
    const fetchMock = mockApi(answer(invoice()));
    renderAt("/companies/c1/purchases");
    const file = xml();
    const sheet = await readInvoice(file);

    expect(sheet.getByText("Torrefação Serra Ltda").closest("p")).toHaveTextContent("12345678000195");
    expect(sheet.getByText(/NF-e nº 123, série 1/)).toHaveTextContent("R$ 300,00");
    expect(sheet.getByText(/NF-e nº 123, série 1/)).toHaveTextContent("05/11/2026");
    const parcels = within(sheet.getByRole("list", { name: "Parcelas da nota" }));
    expect(parcels.getByText("05/12/2026 · R$ 150,00")).toBeInTheDocument();
    expect(parcels.getByText("05/01/2027 · R$ 150,00")).toBeInTheDocument();
    const first = sheet.getByRole("row", { name: /GRAO/ });
    expect(first).toHaveTextContent("10 KG");
    expect(first).toHaveTextContent("R$ 270,50");
    expect(first).toHaveTextContent("R$ 20,50");
    expect(within(first).getByLabelText("Produto do item 1")).toHaveValue("pr1");
    const second = sheet.getByRole("row", { name: /XYZ-9/ });
    expect(within(second).getByLabelText("Produto do item 2")).toHaveValue("");
    expect(within(second).getByText("Sem produto")).toBeInTheDocument();
    // The invoice gives installments, so the payment term does not apply.
    expect(sheet.queryByLabelText("Prazo de pagamento")).not.toBeInTheDocument();
    expect(sheet.getByRole("button", { name: "Entrar compra" })).toBeDisabled();

    // The XML goes to the preview as it is, as the body.
    const [, init] = calls(fetchMock, "POST")[0];
    expect(init?.body).toBe(file);
    expect(new Headers(init?.headers).get("Content-Type")).toBe("application/xml");
  });

  it("enters the purchase with the chosen products, category and day, the file as the body", async () => {
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

    await userEvent.selectOptions(sheet.getByLabelText("Produto do item 2"), "pr2");
    await userEvent.selectOptions(sheet.getByLabelText("Categoria das contas a pagar"), "cat1");
    fireEvent.change(sheet.getByLabelText("Dia em que a mercadoria chegou"), { target: { value: "2026-11-08" } });
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
    await userEvent.selectOptions(sheet.getByLabelText("Prazo de pagamento"), "t1");
    await userEvent.selectOptions(sheet.getByLabelText("Categoria das contas a pagar"), "cat1");
    await userEvent.click(sheet.getByRole("button", { name: "Entrar compra" }));

    expect(await screen.findByText("Compra nº 8 criada a partir da NF-e.")).toBeInTheDocument();
    const url = new URL(String(calls(fetchMock, "POST")[1][0]), "http://localhost");
    expect(url.searchParams.get("payment_term_id")).toBe("t1");
  });

  it("does not let an invoice from an unregistered supplier be entered", async () => {
    mockApi(answer(invoice({ supplier_id: null, items: invoice().items.map((item) => ({ ...item, product_id: "pr1" })) })));
    renderAt("/companies/c1/purchases");
    const sheet = await readInvoice();

    expect(sheet.getByText(/Fornecedor não cadastrado.*12345678000195/)).toBeInTheDocument();
    await userEvent.selectOptions(sheet.getByLabelText("Categoria das contas a pagar"), "cat1");
    expect(sheet.getByRole("button", { name: "Entrar compra" })).toBeDisabled();
  });

  it("says when the invoice was already entered, with a link to the purchases, and does not enter it again", async () => {
    mockApi(
      answer(invoice({ entered: { purchase_id: "o1", number: 7, status: "received" }, items: invoice().items.map((item) => ({ ...item, product_id: "pr1" })) })),
    );
    renderAt("/companies/c1/purchases");
    const sheet = await readInvoice();

    expect(sheet.getByText("Esta nota já foi entrada como a compra nº 7.")).toBeInTheDocument();
    expect(sheet.getByRole("link", { name: "Ver nas compras" })).toHaveAttribute("href", "/companies/c1/purchases");
    expect(sheet.getByRole("button", { name: "Entrar compra" })).toBeDisabled();
  });

  it("shows the API's message for a file it refuses", async () => {
    mockApi(answer({ status: 422, body: { error: "not a valid XML" } }));
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Entrar NF-e (XML)" }));
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
    await userEvent.click(await screen.findByRole("button", { name: "Entrar NF-e (XML)" }));
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
    await userEvent.click(await screen.findByRole("button", { name: "Entrar NF-e (XML)" }));
    const sheet = within((await screen.findByRole("heading", { name: "Entrada por XML (NF-e)" })).closest("aside")!);
    expect(sheet.getByText(/A NF-e fica salva no seu computador/)).toBeInTheDocument();
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
    await sheet.findByRole("button", { name: "Entrar compra" });
    expect(calls(fetchMock, "POST")[0][1]?.body).toBe(file);
  });

  it("offers no entry from an NF-e to a role that only reads", async () => {
    mockApi(answer(invoice(), readOnly));
    renderAt("/companies/c1/purchases");
    await screen.findByRole("heading", { name: "Compras" });
    expect(screen.queryByRole("button", { name: "Entrar NF-e (XML)" })).not.toBeInTheDocument();
  });

  it("shows the access key of an order that came from an NF-e in its details", async () => {
    mockApi(answer(invoice(), (url) => (url.endsWith("/receipts") ? { status: 200, body: { items: [] } } : undefined), [order({ invoice_key: KEY })]));
    renderAt("/companies/c1/purchases");
    await userEvent.click(await screen.findByRole("button", { name: "Detalhes da compra 7" }));
    expect(await screen.findByText(`NF-e (chave): ${KEY}`)).toBeInTheDocument();
  });
});
