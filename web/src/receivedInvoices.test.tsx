import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { ReceivedInvoice } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const invoice = (over: Partial<ReceivedInvoice> = {}): ReceivedInvoice => ({
  id: "r1",
  access_key: "35261112345678000195550010000000011100000010",
  issuer_name: "Fornecedor 1 Ltda",
  issuer_document: "12345678000195",
  total: "150.5",
  issued_on: "2026-11-02",
  situation: "authorized",
  manifestation: "none",
  justification: null,
  ...over,
});

function serve(items: ReceivedInvoice[], posts: Record<string, (body: unknown) => Reply> = {}, me = profile) {
  return mockApi((url, init): Reply => {
    const path = url.replace("/api/companies/c1", "");
    if (url === "/api/auth/me") return { status: 200, body: me };
    if (init?.method === "POST" && path in posts) return posts[path](init.body ? JSON.parse(String(init.body)) : null);
    if (path.startsWith("/fiscal/received-invoices")) return { status: 200, body: { items, next_cursor: null } };
    return { status: 404, body: { error: "not found" } };
  });
}

const postsOf = (fetchMock: ReturnType<typeof mockApi>) =>
  fetchMock.mock.calls
    .filter(([, init]) => init?.method === "POST")
    .map(([url, init]) => [String(url).replace("/api/companies/c1", ""), init?.body ? JSON.parse(String(init.body)) : null]);

it("lists the invoices with their issuer, total, situation and what was declared, and narrows by the declaration", async () => {
  const fetchMock = serve([invoice(), invoice({ id: "r2", issuer_name: "Outro Ltda", manifestation: "awareness" })]);
  renderAt("/companies/c1/fiscal/received");

  const table = within(await screen.findByRole("table"));
  expect(table.getAllByText(/Fornecedor 1 Ltda/).length).toBeGreaterThan(0);
  expect(table.getAllByText("Autorizada")).toHaveLength(2);
  expect(table.getByText("Sem manifestação")).toBeInTheDocument();
  expect(table.getByText("Ciência da operação")).toBeInTheDocument();
  expect(table.getAllByText("R$ 150,50")).toHaveLength(2);

  await userEvent.selectOptions(screen.getByLabelText("Manifestação"), "awareness");
  await screen.findByRole("table");
  expect(fetchMock.mock.calls.some(([url]) => String(url).includes("manifestation=awareness"))).toBe(true);
});

it("fetches from the gateway on request and says how many are new", async () => {
  const fetchMock = serve([], { "/fiscal/received-invoices/sync": () => ({ status: 200, body: { added: 3 } }) });
  renderAt("/companies/c1/fiscal/received");
  await userEvent.click(await screen.findByRole("button", { name: "Buscar notas" }));

  expect(await screen.findByText("3 notas novas.")).toBeInTheDocument();
  expect(postsOf(fetchMock)).toEqual([["/fiscal/received-invoices/sync", null]]);
});

it("declares awareness, then offers only the final words, and asks for a reason only for an operation not performed", async () => {
  const fetchMock = serve([invoice()], {
    "/fiscal/received-invoices/r1/manifest": (body) => ({
      status: 200,
      body: invoice({ manifestation: (body as { word: ReceivedInvoice["manifestation"] }).word }),
    }),
  });
  renderAt("/companies/c1/fiscal/received");
  await userEvent.click(await screen.findByRole("button", { name: "Manifestar Fornecedor 1 Ltda" }));

  let sheet = within(await screen.findByRole("dialog"));
  expect(sheet.getAllByRole("option").map((option) => option.textContent)).toEqual([
    "Escolha",
    "Ciência da operação",
    "Confirmação da operação",
    "Desconhecimento da operação",
    "Operação não realizada",
  ]);
  expect(sheet.queryByLabelText(/Motivo/)).toBeNull();
  await userEvent.selectOptions(sheet.getByLabelText("Palavra"), "awareness");
  await userEvent.click(sheet.getByRole("button", { name: "Declarar" }));
  expect(await within(screen.getByRole("table")).findByText("Ciência da operação")).toBeInTheDocument();

  // After awareness the final words remain, and the one that has a reason asks for it.
  await userEvent.click(await screen.findByRole("button", { name: "Manifestar Fornecedor 1 Ltda" }));
  sheet = within(await screen.findByRole("dialog"));
  expect(sheet.queryByRole("option", { name: "Ciência da operação" })).toBeNull();
  await userEvent.selectOptions(sheet.getByLabelText("Palavra"), "not_performed");
  await userEvent.type(sheet.getByLabelText(/Motivo \(de 15/), "A mercadoria nunca chegou ao estabelecimento");
  await userEvent.click(sheet.getByRole("button", { name: "Declarar" }));

  await within(screen.getByRole("table")).findByText("Operação não realizada");
  expect(postsOf(fetchMock)).toEqual([
    ["/fiscal/received-invoices/r1/manifest", { word: "awareness" }],
    ["/fiscal/received-invoices/r1/manifest", { word: "not_performed", justification: "A mercadoria nunca chegou ao estabelecimento" }],
  ]);
});

it("offers nothing to declare after the last word or on an invoice that is not authorized, and nothing to a reader", async () => {
  serve([invoice({ id: "r2", manifestation: "confirmation" }), invoice({ id: "r3", situation: "cancelled", issuer_name: "Cancelada SA" })]);
  renderAt("/companies/c1/fiscal/received");
  await within(await screen.findByRole("table")).findByText("Confirmação da operação");
  expect(screen.queryByRole("button", { name: /Manifestar/ })).toBeNull();
});

it("gives a reader the list and nothing to fetch or declare", async () => {
  const reader = { ...profile, companies: [{ id: "c1", name: "Padaria", role: "viewer" as const, permissions: { ...OWNER_PERMISSIONS, fiscal: "read" as const } }] };
  serve([invoice()], {}, reader);
  renderAt("/companies/c1/fiscal/received");
  await screen.findByRole("table");
  expect(screen.queryByRole("button", { name: "Buscar notas" })).toBeNull();
  expect(screen.queryByRole("button", { name: /Manifestar/ })).toBeNull();
});

it("tells what the authority refused", async () => {
  serve([invoice()], {
    "/fiscal/received-invoices/r1/manifest": () => ({ status: 422, body: { error: "the authority refused the manifestation (573): Duplicidade de evento" } }),
  });
  renderAt("/companies/c1/fiscal/received");
  await userEvent.click(await screen.findByRole("button", { name: "Manifestar Fornecedor 1 Ltda" }));
  const sheet = within(await screen.findByRole("dialog"));
  await userEvent.selectOptions(sheet.getByLabelText("Palavra"), "confirmation");
  await userEvent.click(sheet.getByRole("button", { name: "Declarar" }));

  expect(await sheet.findByText(/Duplicidade de evento/)).toBeInTheDocument();
});
