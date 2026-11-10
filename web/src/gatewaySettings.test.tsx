import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { GatewaySettings } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const none: GatewaySettings = {
  kind: null,
  focus_token_homologation_set: false,
  focus_token_production_set: false,
  nfeio_api_key_set: false,
  webhook_secret_set: false,
  nfeio_company_homologation: null,
  nfeio_company_production: null,
  nfeio_search_confirmed: false,
  encryption_available: true,
  server_gateway: "focus",
  webhook_path: "/public/fiscal/c1/webhook",
};

function serve(initial: GatewaySettings, saved?: (body: unknown) => GatewaySettings, me = profile) {
  let current = initial;
  return mockApi((url, init): Reply => {
    const path = url.replace("/api/companies/c1", "");
    if (url === "/api/auth/me") return { status: 200, body: me };
    if (path === "/gateway-settings" && init?.method === "PUT") {
      current = saved ? saved(JSON.parse(String(init.body))) : current;
      return { status: 200, body: current };
    }
    if (path === "/gateway-settings") return { status: 200, body: current };
    if (path === "/gateway-settings/history") return { status: 200, body: [] };
    return { status: 404, body: { error: "not found" } };
  });
}

const puts = (fetchMock: ReturnType<typeof mockApi>) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === "PUT").map(([, init]) => JSON.parse(String(init?.body)));

it("says whether each secret is set and never shows one; the server's own gateway is named while the company has none", async () => {
  serve({ ...none, kind: "focus", focus_token_production_set: true });
  renderAt("/companies/c1/gateway");

  const production = await screen.findByLabelText("Token do Focus NFe (produção)");
  expect(production).toHaveValue("");
  expect(production).toHaveAttribute("type", "password");
  expect(production).toHaveAttribute("placeholder", "Configurado: digite para trocar");
  expect(screen.getByLabelText("Token do Focus NFe (homologação)")).toHaveAttribute("placeholder", "Não configurado");
  expect(screen.getByText(/\/public\/fiscal\/c1\/webhook/)).toBeInTheDocument();
});

it("warns that the server's gateway is the one in use until the company sets its own", async () => {
  serve(none);
  renderAt("/companies/c1/gateway");
  expect(await screen.findByText(/vale o do servidor \(focus\)/)).toBeInTheDocument();
});

it("keeps what is not typed, sets what is typed and clears what is ticked, and forgets what was typed once it is saved", async () => {
  const fetchMock = serve(
    { ...none, kind: "focus", focus_token_production_set: true, webhook_secret_set: true },
    () => ({ ...none, kind: "focus", focus_token_homologation_set: true, focus_token_production_set: true, webhook_secret_set: false }),
  );
  renderAt("/companies/c1/gateway");
  await userEvent.type(await screen.findByLabelText("Token do Focus NFe (homologação)"), "tok_homol_1234567890");
  await userEvent.click(screen.getByLabelText("Remover: Segredo do webhook"));
  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));

  expect(await screen.findByText("Salvo.")).toBeInTheDocument();
  expect(puts(fetchMock)).toEqual([
    {
      kind: "focus",
      focus_token_homologation: { action: "set", value: "tok_homol_1234567890" },
      focus_token_production: { action: "keep" },
      nfeio_api_key: { action: "keep" },
      webhook_secret: { action: "clear" },
      nfeio_company_homologation: null,
      nfeio_company_production: null,
      nfeio_search_confirmed: false,
    },
  ]);
  // What was typed is not in the page any more, nor anywhere in it.
  expect(screen.getByLabelText("Token do Focus NFe (homologação)")).toHaveValue("");
  expect(document.body.innerHTML).not.toContain("tok_homol_1234567890");
});

it("asks for the NFE.io fields only when that gateway is chosen", async () => {
  const fetchMock = serve(none, (body) => ({ ...none, kind: "nfeio", nfeio_api_key_set: true, nfeio_company_homologation: (body as { nfeio_company_homologation: string }).nfeio_company_homologation }));
  renderAt("/companies/c1/gateway");
  expect(await screen.findByLabelText("Token do Focus NFe (produção)")).toBeInTheDocument();
  expect(screen.queryByLabelText("Chave da API do NFE.io")).toBeNull();

  await userEvent.selectOptions(screen.getByLabelText("Gateway"), "nfeio");
  expect(screen.queryByLabelText("Token do Focus NFe (produção)")).toBeNull();
  await userEvent.type(screen.getByLabelText("Chave da API do NFE.io"), "key_1234567890");
  await userEvent.type(screen.getByLabelText("Empresa no NFE.io (homologação)"), "company-123");
  await userEvent.click(screen.getByLabelText(/Busca por referência confirmada/));
  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));

  await screen.findByText("Salvo.");
  expect(puts(fetchMock)[0]).toMatchObject({
    kind: "nfeio",
    nfeio_api_key: { action: "set", value: "key_1234567890" },
    nfeio_company_homologation: "company-123",
    nfeio_search_confirmed: true,
  });
});

it("says when the server cannot seal secrets, and shows an error the server refuses with", async () => {
  serve({ ...none, encryption_available: false });
  renderAt("/companies/c1/gateway");
  expect(await screen.findByText(/FISCAL_SECRETS_KEY/)).toBeInTheDocument();
});

it("shows the error of a refusal without the typed value in it", async () => {
  mockApi((url, init): Reply => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/gateway-settings") && init?.method === "PUT") return { status: 422, body: { error: "secret of the gateway settings is not valid" } };
    if (url.endsWith("/gateway-settings/history")) return { status: 200, body: [] };
    return { status: 200, body: none };
  });
  renderAt("/companies/c1/gateway");
  await userEvent.type(await screen.findByLabelText("Token do Focus NFe (produção)"), "curto");
  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));
  expect(await screen.findByText(/secret of the gateway settings is not valid/)).toBeInTheDocument();
});

it("shows a reader the state and nothing to change", async () => {
  const reader = { ...profile, companies: [{ id: "c1", name: "Padaria", role: "viewer" as const, permissions: { ...OWNER_PERMISSIONS, members: "read" as const } }] };
  serve({ ...none, kind: "focus" }, undefined, reader);
  renderAt("/companies/c1/gateway");
  expect(await screen.findByLabelText("Token do Focus NFe (produção)")).toBeDisabled();
  expect(screen.queryByRole("button", { name: "Salvar" })).toBeNull();
  expect(within(screen.getByRole("main")).queryByRole("button", { name: /Remover/ })).toBeNull();
});
