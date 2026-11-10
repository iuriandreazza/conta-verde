import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, afterEach, vi } from "vitest";
import type { FiscalDocument, FiscalEvent, FiscalSettings } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const settings = {
  natures: [
    { code: "venda", name: "Venda de mercadoria", kind: "product", cfop_in_state: "5102", cfop_interstate: "6102", active: true, rules: [] },
    { code: "servico", name: "Prestação de serviço", kind: "service", cfop_in_state: null, cfop_interstate: null, active: true, rules: [] },
  ],
} as unknown as FiscalSettings;

const document = (over: Partial<FiscalDocument> = {}): FiscalDocument => ({
  id: "d1",
  kind: "nfe",
  status: "authorized",
  cancellation: null,
  sale_id: "s1",
  issuer_cnpj: "11222333000181",
  issuer_name: "Verde Comércio Ltda",
  environment: "production",
  recipient_name: "Maria Souza",
  recipient_document: "52998224725",
  nature_code: "venda",
  total: "35",
  issued_on: "2026-11-09",
  access_key: "35261111222333000181550010000000071123456780",
  number: 7,
  series: 1,
  protocol: "135260000000001",
  nfse: null,
  dps: null,
  refusal: null,
  note: null,
  submitted_by: "Ana",
  files: ["xml", "pdf"],
  ...over,
});

const letter = (over: Partial<FiscalEvent> = {}): FiscalEvent => ({
  id: "e1",
  kind: "correction",
  status: "accepted",
  text: "Corrige o endereço de entrega",
  sequence: 1,
  series: null,
  first: null,
  last: null,
  protocol: null,
  refusal: null,
  requested_by: "Ana",
  has_file: false,
  ...over,
});

/** The API: a list of documents, the detail of each, and what a POST answers with. */
function serve(documents: FiscalDocument[], me = profile, posts: Record<string, unknown> = {}) {
  return mockApi((url, init) => {
    // A document a POST answers with is what the server holds from then on.
    const answer = init?.method === "POST" ? (posts[url.replace("/api/companies/c1", "")] as FiscalDocument | undefined) : undefined;
    if (answer && "files" in answer) {
      const place = documents.findIndex((item) => item.id === answer.id);
      if (place >= 0) documents[place] = answer;
    }
    const path = url.replace("/api/companies/c1", "");
    if (url === "/api/auth/me") return { status: 200, body: me };
    if (path === "/fiscal/settings") return { status: 200, body: settings };
    if (path.startsWith("/sales")) {
      return { status: 200, body: { items: [{ id: "s1", number: 12, total: "35" }], next_cursor: null } };
    }
    if (init?.method === "POST" && path in posts) return { status: 200, body: posts[path] };
    if (path.startsWith("/fiscal/documents") && path.endsWith("/corrections")) return { status: 200, body: [letter()] };
    if (path.endsWith("/history")) return { status: 200, body: [] };
    if (path === "/fiscal/invalidations") return { status: 200, body: [] };
    const one = /^\/fiscal\/documents\/(\w+)$/.exec(path);
    if (one) return { status: 200, body: documents.find((item) => item.id === one[1]) };
    // Like the server, the list does not carry the files: the detail does.
    if (path.startsWith("/fiscal/documents")) return { status: 200, body: { items: documents.map((item) => ({ ...item, files: [] })), next_cursor: null } };
    return { status: 404, body: { error: "not found" } };
  });
}

const postsOf = (fetchMock: ReturnType<typeof mockApi>) =>
  fetchMock.mock.calls
    .filter(([, init]) => init?.method === "POST")
    .map(([url, init]) => [String(url).replace("/api/companies/c1", ""), JSON.parse(String(init?.body ?? "null"))]);

it("lists the documents with their number, recipient and situation", async () => {
  serve([document(), document({ id: "d2", status: "rejected", number: null, series: null, access_key: null, files: [] })]);
  renderAt("/companies/c1/fiscal/documents");

  const table = within(await screen.findByRole("table"));
  expect(table.getByText("1/7")).toBeInTheDocument();
  expect(table.getAllByText("Maria Souza")).toHaveLength(2);
  expect(table.getByText("Autorizada")).toBeInTheDocument();
  expect(table.getByText("Rejeitada")).toBeInTheDocument();
});

it("issues only with every tax choice made by the person: nothing is filled in for them", async () => {
  // The list the page reloads after issuing already holds the new document.
  const fetchMock = serve([document({ status: "processing", files: [] })], profile, {
    "/fiscal/documents": document({ status: "processing", files: [] }),
  });
  renderAt("/companies/c1/fiscal/documents");
  await userEvent.click(await screen.findByRole("button", { name: "Emitir NF-e" }));
  const sheet = within(await screen.findByRole("dialog"));

  for (const label of ["Venda", "Natureza de operação", "Origem da mercadoria", "Presença do comprador", "Forma de pagamento", "Consumidor final"]) {
    expect(await sheet.findByLabelText(label)).toHaveValue("");
  }
  // A service nature is not offered for an NF-e.
  expect(sheet.queryByRole("option", { name: "Prestação de serviço" })).toBeNull();

  await userEvent.selectOptions(sheet.getByLabelText("Venda"), "s1");
  await userEvent.selectOptions(sheet.getByLabelText("Natureza de operação"), "venda");
  await userEvent.selectOptions(sheet.getByLabelText("Origem da mercadoria"), "0");
  await userEvent.selectOptions(sheet.getByLabelText("Presença do comprador"), "internet");
  await userEvent.selectOptions(sheet.getByLabelText("Forma de pagamento"), "pix");
  await userEvent.selectOptions(sheet.getByLabelText("Consumidor final"), "yes");
  await userEvent.click(sheet.getByRole("button", { name: "Emitir" }));

  await screen.findByRole("button", { name: "Atualizar situação" });
  expect(postsOf(fetchMock)).toEqual([
    [
      "/fiscal/documents",
      { sale_id: "s1", nature_code: "venda", origin: 0, presence: "internet", payment: "pix", final_consumer: true },
    ],
  ]);
});

it("shows what the authority said, offers the files and warns that a test document has no fiscal value", async () => {
  serve([document({ environment: "homologation", note: "autorizada depois do fechamento" })]);
  renderAt("/companies/c1/fiscal/documents");
  await userEvent.click(await screen.findByRole("button", { name: "1/7" }));

  expect(await screen.findByText("35261111222333000181550010000000071123456780")).toBeInTheDocument();
  expect(screen.getByText(/não tem valor fiscal/)).toBeInTheDocument();
  expect(screen.getByText("autorizada depois do fechamento")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Baixar XML" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Baixar DANFE (PDF)" })).toBeInTheDocument();
  expect(await screen.findByText(/Corrige o endereço de entrega — Aceita/)).toBeInTheDocument();
});

it("cancels with a reason and sends a correction letter", async () => {
  const fetchMock = serve([document()], profile, {
    "/fiscal/documents/d1/cancel": document({ status: "cancelled", files: ["xml", "pdf", "cancellation_xml"] }),
    "/fiscal/documents/d1/corrections": letter({ id: "e2", sequence: 2 }),
  });
  renderAt("/companies/c1/fiscal/documents");
  await userEvent.click(await screen.findByRole("button", { name: "1/7" }));

  const sheet = within(await screen.findByRole("dialog"));
  await userEvent.type(await sheet.findByLabelText(/Texto \(de 15/), "Corrige o endereço de entrega do destinatário");
  await userEvent.click(sheet.getByRole("button", { name: "Enviar carta" }));
  await userEvent.type(sheet.getByLabelText(/Motivo \(de 15/), "Pedido feito em duplicidade");
  await userEvent.click(sheet.getByRole("button", { name: "Cancelar nota" }));

  expect(await sheet.findByText("Cancelada", { selector: ".badge" })).toBeInTheDocument();
  expect(postsOf(fetchMock)).toEqual([
    ["/fiscal/documents/d1/corrections", { text: "Corrige o endereço de entrega do destinatário" }],
    ["/fiscal/documents/d1/cancel", { reason: "Pedido feito em duplicidade" }],
  ]);
});

it("says a cancellation is waiting for the authority and offers no second one, only to give up on it", async () => {
  const waiting = document({ cancellation: letter({ id: "c1", kind: "cancellation", status: "sent", text: "Pedido feito em duplicidade", sequence: null }) });
  const fetchMock = serve([waiting], profile, { "/fiscal/events/c1/abandon": letter({ id: "c1", kind: "cancellation", status: "refused", sequence: null }) });
  renderAt("/companies/c1/fiscal/documents");
  await userEvent.click(await screen.findByRole("button", { name: "1/7" }));

  const sheet = within(await screen.findByRole("dialog"));
  expect(await sheet.findByText(/O cancelamento foi enviado e a SEFAZ ainda não respondeu/)).toBeInTheDocument();
  expect(sheet.queryByRole("button", { name: "Cancelar nota" })).toBeNull();
  await userEvent.click(sheet.getByRole("button", { name: "Desistir do cancelamento" }));
  expect(postsOf(fetchMock)).toEqual([["/fiscal/events/c1/abandon", null]]);
});

it("says why a document was rejected and sends it again with the choices made again", async () => {
  const fetchMock = serve(
    [document({ status: "rejected", refusal: { code: "225", message: "Falha no Schema XML" }, number: null, series: null, access_key: null, files: [] })],
    profile,
    { "/fiscal/documents/d1/resend": document({ status: "processing", files: [] }) },
  );
  renderAt("/companies/c1/fiscal/documents");
  await userEvent.click(await screen.findByRole("button", { name: "—" }));

  expect(await screen.findByText(/Recusada \(225\): Falha no Schema XML/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Reenviar" }));
  const form = screen.getByRole("button", { name: "Reenviar a mesma nota" }).closest("form")!;
  await userEvent.selectOptions(within(form).getByLabelText("Natureza de operação"), "venda");
  await userEvent.selectOptions(within(form).getByLabelText("Origem da mercadoria"), "1");
  await userEvent.selectOptions(within(form).getByLabelText("Presença do comprador"), "in_person");
  await userEvent.selectOptions(within(form).getByLabelText("Forma de pagamento"), "cash");
  await userEvent.selectOptions(within(form).getByLabelText("Consumidor final"), "no");
  await userEvent.click(within(form).getByRole("button", { name: "Reenviar a mesma nota" }));

  await within(screen.getByRole("dialog")).findByText("Processando", { selector: ".badge" });
  expect(postsOf(fetchMock)).toEqual([
    [
      "/fiscal/documents/d1/resend",
      { sale_id: "s1", nature_code: "venda", origin: 1, presence: "in_person", payment: "cash", final_consumer: false },
    ],
  ]);
});

it("declares a range of numbers unused with a reason", async () => {
  const fetchMock = serve([], profile, {
    "/fiscal/invalidations": { ...letter(), kind: "invalidation", sequence: null, series: 1, first: 10, last: 12 },
  });
  renderAt("/companies/c1/fiscal/documents");
  await screen.findByText(/Nenhum documento fiscal ainda/);

  await userEvent.type(screen.getByLabelText("Do número"), "10");
  await userEvent.type(screen.getByLabelText("Até o número"), "12");
  await userEvent.type(screen.getByLabelText(/Motivo \(de 15/), "Numeração pulada por falha do sistema");
  await userEvent.click(screen.getByRole("button", { name: "Inutilizar" }));

  expect(postsOf(fetchMock)).toEqual([
    ["/fiscal/invalidations", { series: 1, first: 10, last: 12, reason: "Numeração pulada por falha do sistema" }],
  ]);
});

it("lets whoever only reads see the documents and download, but not issue, cancel or correct", async () => {
  const reader = { ...profile, companies: [{ id: "c1", name: "Padaria", role: "viewer" as const, permissions: { ...OWNER_PERMISSIONS, fiscal: "read" as const } }] };
  serve([document()], reader);
  renderAt("/companies/c1/fiscal/documents");
  await userEvent.click(await screen.findByRole("button", { name: "1/7" }));

  expect(screen.queryByRole("button", { name: "Emitir NF-e" })).toBeNull();
  expect(await screen.findByRole("button", { name: "Baixar XML" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Cancelar nota" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Enviar carta" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Inutilizar" })).toBeNull();
});
