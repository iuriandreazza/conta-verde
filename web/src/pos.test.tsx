import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { FiscalDocument, FiscalSettings, Party, Product } from "./api";
import { mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const customer: Party = {
  id: "p1",
  company_id: "c1",
  name: "Maria Souza",
  document: "52998224725",
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
const coffee: Product = {
  id: "pr1",
  company_id: "c1",
  kind: "product",
  name: "Café 500g",
  sku: "cafe",
  unit: "UN",
  ncm: "09012100",
  sale_price: "10",
  cost_price: null,
  minimum_stock: null,
  active: true,
};
const consulting: Product = { ...coffee, id: "pr2", kind: "service", name: "Consultoria", ncm: null };
const revenue = { id: "cat1", company_id: "c1", parent_id: null, name: "Vendas", dre_group: "gross_revenue", cash_flow_group: "operating", level: 1, active: true };
const settings = {
  natures: [
    { code: "venda", name: "Venda de mercadoria", kind: "product", cfop_in_state: "5102", cfop_interstate: "6102", active: true, rules: [], service: null },
    { code: "servico", name: "Prestação de serviço", kind: "service", cfop_in_state: null, cfop_interstate: null, active: true, rules: [], service: null },
  ],
} as unknown as FiscalSettings;

const invoice: FiscalDocument = {
  id: "d1",
  kind: "nfce",
  status: "authorized",
  cancellation: null,
  sale_id: "s1",
  issuer_cnpj: "11222333000181",
  issuer_name: "Padaria",
  environment: "production",
  recipient_name: "Maria Souza",
  recipient_document: "52998224725",
  nature_code: "venda",
  total: "30",
  issued_on: "2026-11-09",
  access_key: "35261111222333000181650010000000011123456780",
  number: 1,
  series: 1,
  protocol: null,
  nfse: null,
  dps: null,
  refusal: null,
  note: null,
  submitted_by: "Ana",
  files: ["xml"],
};

const serve = (consumer: (calls: number) => Reply, complete: (calls: number) => Reply = () => ({ status: 200, body: { id: "s1" } })) => {
  let issued = 0;
  let completions = 0;
  return mockApi((url, init): Reply => {
    if (init?.method === "POST" && url.endsWith("/fiscal/documents/d0/resend-consumer")) return consumer(++issued);
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (init?.method === "POST" && url.endsWith("/sales")) return { status: 201, body: { id: "s1" } };
    if (init?.method === "POST" && url.endsWith("/sales/s1/complete")) return complete(++completions);
    if (init?.method === "POST" && url.endsWith("/fiscal/consumer-documents")) return consumer(++issued);
    if (url.endsWith("/fiscal/settings")) return { status: 200, body: settings };
    if (url.includes("/parties")) return { status: 200, body: { items: [customer], next_cursor: null } };
    if (url.includes("/products")) return { status: 200, body: { items: [coffee, consulting], next_cursor: null } };
    if (url.includes("/payment-terms")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [revenue] } };
    if (url.includes("/cost-centers")) return { status: 200, body: { items: [], next_cursor: null } };
    if (url.includes("/price-lists")) return { status: 200, body: { items: [], next_cursor: null } };
    return { status: 404, body: { error: "not found" } };
  });
};

const bodyOf = (fetchMock: ReturnType<typeof mockApi>, suffix: string) =>
  fetchMock.mock.calls
    .filter(([url, init]) => init?.method === "POST" && String(url).endsWith(suffix))
    .map(([, init]) => (init?.body ? JSON.parse(String(init.body)) : null));

/** Fills the sale: a customer, a category and a coffee, then the payment, and leaves the fiscal choices to the test. */
async function fillTheSale(paid: string) {
  // What the screen offers is read from the server: wait for it before choosing.
  await screen.findByRole("option", { name: "Maria Souza" });
  await screen.findByRole("option", { name: "Vendas" });
  await userEvent.selectOptions(screen.getByLabelText("Cliente"), "p1");
  await userEvent.selectOptions(screen.getByLabelText("Categoria da receita"), "cat1");
  await userEvent.selectOptions(screen.getByLabelText("Produto ou serviço do item 1"), "pr1");
  await userEvent.clear(screen.getByLabelText("Quantidade do item 1"));
  await userEvent.type(screen.getByLabelText("Quantidade do item 1"), "3");
  await userEvent.selectOptions(screen.getByLabelText("Forma de pagamento"), "pix");
  await userEvent.type(screen.getByLabelText("Valor pago"), paid);
}

async function fillTheChoices() {
  await userEvent.selectOptions(screen.getByLabelText("Natureza de operação"), "venda");
  await userEvent.selectOptions(screen.getByLabelText("Origem da mercadoria"), "0");
  await userEvent.selectOptions(screen.getByLabelText("Presença do comprador"), "in_person");
  await userEvent.selectOptions(screen.getByLabelText("Identificar o consumidor na nota"), "yes");
}

it("starts with no fiscal choice made, offers only products and the natures of products, and waits for the payment to add up", async () => {
  serve(() => ({ status: 200, body: invoice }));
  renderAt("/companies/c1/pos");

  for (const label of ["Natureza de operação", "Origem da mercadoria", "Presença do comprador", "Identificar o consumidor na nota"]) {
    expect(await screen.findByLabelText(label)).toHaveValue("");
  }
  expect(screen.queryByRole("option", { name: /Consultoria/ })).toBeNull();
  // The counter's presences only: not "Pela internet" nor "Por telefone".
  expect(screen.queryByRole("option", { name: "Pela internet" })).toBeNull();
  expect(screen.getByRole("option", { name: "Presencial" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "Prestação de serviço" })).toBeNull();

  await fillTheSale("29.99");
  await fillTheChoices();
  // 3 x 10 is 30: what is paid is 1 cent short, so nothing can be sent yet, and the screen says how much is missing.
  expect(screen.getByRole("button", { name: "Finalizar e emitir NFC-e" })).toBeDisabled();
  expect(await screen.findByText(/Falta R\$ 0,01 a receber/)).toBeInTheDocument();
  await userEvent.clear(screen.getByLabelText("Valor pago"));
  await userEvent.type(screen.getByLabelText("Valor pago"), "30");
  await waitFor(() => expect(screen.getByRole("button", { name: "Finalizar e emitir NFC-e" })).toBeEnabled());
});

it("makes the sale, completes it and issues the NFC-e with what the person chose", async () => {
  const fetchMock = serve(() => ({ status: 200, body: invoice }));
  renderAt("/companies/c1/pos");
  await fillTheSale("30");
  await fillTheChoices();
  await userEvent.click(screen.getByRole("button", { name: "Finalizar e emitir NFC-e" }));

  expect(await screen.findByText(/35261111222333000181650010000000011123456780/)).toBeInTheDocument();
  expect(bodyOf(fetchMock, "/sales")).toHaveLength(1);
  expect(bodyOf(fetchMock, "/sales/s1/complete")).toHaveLength(1);
  expect(bodyOf(fetchMock, "/fiscal/consumer-documents")).toEqual([
    {
      sale_id: "s1",
      nature_code: "venda",
      origin: 0,
      presence: "in_person",
      identify_consumer: true,
      payments: [{ form: "pix", amount: "30" }],
    },
  ]);
  expect(screen.getByRole("button", { name: "Nova venda" })).toBeInTheDocument();
});

it("never makes the sale twice: when the invoice fails the person tries the invoice again", async () => {
  const fetchMock = serve((call) =>
    call === 1 ? { status: 422, body: { error: "the customer has no CPF or CNPJ to be named on the invoice" } } : { status: 200, body: invoice },
  );
  renderAt("/companies/c1/pos");
  await fillTheSale("30");
  await fillTheChoices();
  await userEvent.click(screen.getByRole("button", { name: "Finalizar e emitir NFC-e" }));

  expect(await screen.findByText(/no CPF or CNPJ to be named/)).toBeInTheDocument();
  expect(screen.getByText(/A venda já foi feita e concluída/)).toBeInTheDocument();
  await userEvent.selectOptions(screen.getByLabelText("Identificar o consumidor na nota"), "no");
  await userEvent.click(screen.getByRole("button", { name: "Emitir a NFC-e de novo" }));

  await screen.findByRole("button", { name: "Nova venda" });
  expect(bodyOf(fetchMock, "/sales")).toHaveLength(1);
  expect(bodyOf(fetchMock, "/sales/s1/complete")).toHaveLength(1);
  const issues = bodyOf(fetchMock, "/fiscal/consumer-documents");
  expect(issues).toHaveLength(2);
  expect(issues[1].sale_id).toBe("s1");
  expect(issues[1].identify_consumer).toBe(false);
});

it("when the sale could not be completed, asks to complete it again: the sale is not made twice and its lines are not editable", async () => {
  const fetchMock = serve(
    () => ({ status: 200, body: invoice }),
    (call) => (call === 1 ? { status: 409, body: { error: "not enough stock" } } : { status: 200, body: { id: "s1" } }),
  );
  renderAt("/companies/c1/pos");
  await fillTheSale("30");
  await fillTheChoices();
  await userEvent.click(screen.getByRole("button", { name: "Finalizar e emitir NFC-e" }));

  expect(await screen.findByText(/not enough stock/)).toBeInTheDocument();
  expect(screen.getByText(/A venda foi feita, mas ainda não concluída/)).toBeInTheDocument();
  expect(screen.getByLabelText("Quantidade do item 1")).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Concluir a venda e emitir" }));

  await screen.findByRole("button", { name: "Nova venda" });
  expect(bodyOf(fetchMock, "/sales")).toHaveLength(1);
  expect(bodyOf(fetchMock, "/sales/s1/complete")).toHaveLength(2);
  expect(bodyOf(fetchMock, "/fiscal/consumer-documents")).toHaveLength(1);
});

it("sends a refused NFC-e again under its own id instead of issuing a second one", async () => {
  const rejected: FiscalDocument = { ...invoice, id: "d0", status: "rejected", access_key: null, number: null, series: null, refusal: { code: "E225", message: "Falha no schema" } };
  const fetchMock = serve((call) => ({ status: 200, body: call === 1 ? rejected : invoice }));
  renderAt("/companies/c1/pos");
  await fillTheSale("30");
  await fillTheChoices();
  await userEvent.click(screen.getByRole("button", { name: "Finalizar e emitir NFC-e" }));

  expect(await screen.findByText(/Recusada \(E225\): Falha no schema/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Emitir a NFC-e de novo" }));

  await screen.findByRole("button", { name: "Nova venda" });
  expect(bodyOf(fetchMock, "/sales")).toHaveLength(1);
  expect(bodyOf(fetchMock, "/fiscal/consumer-documents")).toHaveLength(1);
  expect(bodyOf(fetchMock, "/fiscal/documents/d0/resend-consumer")).toHaveLength(1);
});

it("does not take a payment of nothing as a payment", async () => {
  serve(() => ({ status: 200, body: invoice }));
  renderAt("/companies/c1/pos");
  await fillTheSale("0");
  await fillTheChoices();
  expect(screen.getByRole("button", { name: "Finalizar e emitir NFC-e" })).toBeDisabled();
});
