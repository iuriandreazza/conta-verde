import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { ApiKey } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt } from "./testSupport";

const key = (overrides: Partial<ApiKey> = {}): ApiKey => ({
  id: "k1",
  name: "Loja online",
  label: "cvk_1a2b3c",
  scopes: ["parties:read", "sales:write"],
  status: "active",
  expires_at: null,
  created_at: 1_800_000_000_000,
  created_by: "Ana",
  rotated_from: null,
  last_used_on: null,
  ...overrides,
});

const SECRET = "cvk_" + "a".repeat(64);

beforeEach(() => {
  vi.restoreAllMocks();
  Object.defineProperty(navigator, "clipboard", { value: { writeText: vi.fn(async () => undefined) }, configurable: true });
});
afterEach(() => vi.unstubAllGlobals());

describe("the API keys page", () => {
  it("lists the keys with what each can do, how it stands and when it was last used", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      return {
        status: 200,
        body: {
          items: [
            key({ last_used_on: "2026-11-09" }),
            key({ id: "k2", name: "Antiga", label: "cvk_ffffff", scopes: ["products:read"], status: "revoked" }),
          ],
        },
      };
    });
    renderAt("/companies/c1/api-keys");

    expect(await screen.findByText("Loja online")).toBeInTheDocument();
    expect(screen.getByText("Clientes e fornecedores: só consulta")).toBeInTheDocument();
    expect(screen.getByText("Vendas: consulta e altera")).toBeInTheDocument();
    expect(screen.getByText("Usada em 09/11/2026")).toBeInTheDocument();
    expect(screen.getByText("Ativa")).toBeInTheDocument();
    expect(screen.getByText("Revogada")).toBeInTheDocument();
    expect(screen.getByText("Nunca usada")).toBeInTheDocument();
    // A revoked key offers its history and nothing that changes it.
    expect(screen.queryByRole("button", { name: "Girar Antiga" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Revogar Antiga" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Histórico de Antiga" })).toBeInTheDocument();
  });

  it("makes a key from the scopes chosen and shows its secret once, with the address to use it at", async () => {
    let created: { name: string; scopes: string[]; expires_in_hours?: number } | null = null;
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url === "/api/companies/c1/api-keys" && init?.method === "POST") {
        created = JSON.parse(String(init.body));
        return { status: 201, body: { key: key({ name: "Minha loja", scopes: ["sales:write"] }), secret: SECRET } };
      }
      return { status: 200, body: { items: [] } };
    });
    renderAt("/companies/c1/api-keys");
    await screen.findByText("Nenhuma chave ainda.");

    await userEvent.click(screen.getByRole("button", { name: "Nova chave" }));
    const sheet = await screen.findByRole("dialog");
    await userEvent.type(within(sheet).getByLabelText("Nome"), "Minha loja");
    // Nothing chosen yet: it says so and sends nothing.
    await userEvent.click(within(sheet).getByRole("button", { name: "Criar chave" }));
    expect(await within(sheet).findByText("Escolha o que a chave pode fazer em pelo menos uma área.")).toBeInTheDocument();
    expect(created).toBeNull();

    await userEvent.selectOptions(within(sheet).getByLabelText("Vendas"), "Consulta e altera");
    await userEvent.selectOptions(within(sheet).getByLabelText("Validade"), "30 dias");
    await userEvent.click(within(sheet).getByRole("button", { name: "Criar chave" }));

    expect(await screen.findByDisplayValue(SECRET)).toBeInTheDocument();
    expect(created).toEqual({ name: "Minha loja", scopes: ["sales:write"], expires_in_hours: 720 });
    expect(screen.getByText(/ela não aparece de novo/)).toBeInTheDocument();
    expect(screen.getByDisplayValue(`${window.location.origin}/api/v1`)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Copiar a chave de Minha loja" }));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(SECRET);
  });

  it("never offers an area a key may not have", async () => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [] } }));
    renderAt("/companies/c1/api-keys");
    await userEvent.click(await screen.findByRole("button", { name: "Nova chave" }));
    const sheet = await screen.findByRole("dialog");
    for (const area of ["Clientes e fornecedores", "Produtos e serviços", "Financeiro", "Vendas", "Compras e estoque"]) {
      expect(within(sheet).getByLabelText(area)).toBeInTheDocument();
    }
    for (const forbidden of ["Membros", "Contador", "Contabilidade", "Fiscal"]) {
      expect(within(sheet).queryByLabelText(forbidden)).not.toBeInTheDocument();
    }
  });

  it("rotates a key after asking how long the old one goes on, and shows the new secret once", async () => {
    let rotated: unknown = null;
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url === "/api/companies/c1/api-keys/k1/rotate") {
        rotated = JSON.parse(String(init?.body));
        return { status: 201, body: { key: key({ id: "k3", rotated_from: "k1" }), secret: SECRET } };
      }
      return { status: 200, body: { items: [key()] } };
    });
    renderAt("/companies/c1/api-keys");
    await userEvent.click(await screen.findByRole("button", { name: "Girar Loja online" }));
    await userEvent.selectOptions(screen.getByLabelText("A chave antiga continua valendo"), "7 dias");
    await userEvent.click(screen.getByRole("button", { name: "Confirmar a rotação de Loja online" }));

    expect(await screen.findByDisplayValue(SECRET)).toBeInTheDocument();
    expect(rotated).toEqual({ grace_hours: 168 });
  });

  it("revokes a key only after it is confirmed, and says when the answer is a refusal", async () => {
    let calls = 0;
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url === "/api/companies/c1/api-keys/k1/revoke") {
        calls += 1;
        return calls === 1 ? { status: 404, body: { error: "API key not found" } } : { status: 200, body: key({ status: "revoked" }) };
      }
      void init;
      return { status: 200, body: { items: [key()] } };
    });
    renderAt("/companies/c1/api-keys");
    await userEvent.click(await screen.findByRole("button", { name: "Revogar Loja online" }));
    expect(calls).toBe(0);
    await userEvent.click(screen.getByRole("button", { name: "Confirmar a revogação de Loja online" }));
    expect(await screen.findByText("API key not found")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "Revogar Loja online" })).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "Revogar Loja online" }));
    await userEvent.click(screen.getByRole("button", { name: "Confirmar a revogação de Loja online" }));
    await waitFor(() => expect(calls).toBe(2));
  });

  it("shows what a key did to someone who may only read", async () => {
    const reader = { ...profile, companies: [{ ...profile.companies[0], permissions: { ...OWNER_PERMISSIONS, members: "read" as const } }] };
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: reader };
      if (url.endsWith("/history")) {
        return { status: 200, body: [{ at: 1_800_000_000_000, actor: { kind: "user", id: "u1", name: "Ana" }, action: "created", changes: [{ field: "scopes", from: null, to: "sales:write" }] }] };
      }
      return { status: 200, body: { items: [key()] } };
    });
    renderAt("/companies/c1/api-keys");
    await screen.findByText("Loja online");
    expect(screen.queryByRole("button", { name: "Nova chave" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Girar Loja online" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Histórico de Loja online" }));
    expect(await screen.findByText("Escopos:")).toBeInTheDocument();
  });
});
