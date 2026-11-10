import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { BankAccount, BankConnection } from "./api";
import { mockApi, profile, renderAt } from "./testSupport";

const account = (id: string, name: string, overrides: Partial<BankAccount> = {}): BankAccount => ({
  id,
  company_id: "c1",
  kind: "checking",
  name,
  bank_code: null,
  branch: null,
  number: null,
  active: true,
  ...overrides,
});

const connection = (overrides: Partial<BankConnection> = {}): BankConnection => ({
  id: "k1",
  bank_account_id: "a1",
  provider: "pluggy",
  item_id: "item-1",
  account_id: "acc-1",
  status: "connected",
  last_synced_on: "2026-11-09",
  last_error: null,
  connected_by: "Ana",
  created_at: 1_800_000_000_000,
  ...overrides,
});

const ACCOUNTS = { items: [account("a1", "Caixa"), account("a2", "Banco do Brasil"), account("a3", "Antiga", { active: false })], next_cursor: null };

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

describe("the bank connections page", () => {
  it("lists what is connected with its state, last pull and why it stopped, and says when no provider is configured", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/bank-accounts")) return { status: 200, body: ACCOUNTS };
      return {
        status: 200,
        body: {
          items: [connection(), connection({ id: "k2", bank_account_id: "a2", status: "needs_attention", last_error: "the bank login failed", last_synced_on: null })],
          available: false,
          provider: "none",
        },
      };
    });
    renderAt("/companies/c1/bank-connections");

    expect(await screen.findByText("Caixa")).toBeInTheDocument();
    expect(screen.getByText("Conectada")).toBeInTheDocument();
    expect(screen.getByText("09/11/2026")).toBeInTheDocument();
    expect(screen.getByText("Precisa de você")).toBeInTheDocument();
    expect(screen.getByText("the bank login failed")).toBeInTheDocument();
    expect(screen.getByText("Ainda não sincronizou")).toBeInTheDocument();
    expect(screen.getByText(/Nenhum provedor de dados bancários está configurado/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Conectar" })).not.toBeInTheDocument();
  });

  it("looks up the accounts of a connection, offers only free active bank accounts and connects one", async () => {
    let sent: unknown = null;
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/bank-accounts")) return { status: 200, body: ACCOUNTS };
      if (url.includes("/bank-connections/accounts?item_id=item-9")) {
        return { status: 200, body: { items: [{ id: "acc-9", name: "Conta Corrente", kind: "BANK", number: "1234-5" }, { id: "acc-8", name: "Poupança", kind: "BANK", number: null }] } };
      }
      if (url === "/api/companies/c1/bank-connections" && init?.method === "POST") {
        sent = JSON.parse(String(init.body));
        return { status: 201, body: connection({ id: "k3", bank_account_id: "a2", item_id: "item-9", account_id: "acc-9" }) };
      }
      return { status: 200, body: { items: [connection()], available: true, provider: "pluggy" } };
    });
    renderAt("/companies/c1/bank-connections");
    await screen.findByText("Conectar uma conta");

    // "Caixa" is taken by a live connection and "Antiga" is inactive: only the free one is offered.
    const select = screen.getByLabelText("Conta bancária no sistema");
    expect(within(select).queryByRole("option", { name: "Caixa" })).not.toBeInTheDocument();
    expect(within(select).queryByRole("option", { name: "Antiga" })).not.toBeInTheDocument();
    await userEvent.selectOptions(select, "a2");
    await userEvent.type(screen.getByLabelText("Código da conexão (Item ID)"), "item-9");
    expect(screen.getByRole("button", { name: "Conectar" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Buscar contas" }));
    await userEvent.selectOptions(await screen.findByLabelText("Conta no provedor"), "acc-9");
    await userEvent.click(screen.getByRole("button", { name: "Conectar" }));

    await waitFor(() => expect(sent).toEqual({ bank_account_id: "a2", item_id: "item-9", account_id: "acc-9" }));
    expect(await screen.findByText(/Conta conectada/)).toBeInTheDocument();
  });

  it("pulls the statement now and says what came in, and ends a connection only after a second click", async () => {
    const calls: string[] = [];
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/bank-accounts")) return { status: 200, body: ACCOUNTS };
      if (url.endsWith("/sync")) {
        calls.push("sync");
        return { status: 200, body: { created: 2, skipped: 1, refused: 1, needs_attention: false } };
      }
      if (url.endsWith("/disconnect")) {
        calls.push(`${init?.method} disconnect`);
        return { status: 200, body: connection({ status: "disconnected" }) };
      }
      return { status: 200, body: { items: [connection()], available: true, provider: "pluggy" } };
    });
    renderAt("/companies/c1/bank-connections");
    await userEvent.click(await screen.findByRole("button", { name: "Sincronizar Caixa" }));
    expect(await screen.findByText("Extrato sincronizado: 2 novas, 1 já conhecida, 1 que não deu para ler.")).toBeInTheDocument();
    expect(calls).toEqual(["sync"]);

    await userEvent.click(screen.getByRole("button", { name: "Desconectar Caixa" }));
    expect(calls).toEqual(["sync"]);
    await userEvent.click(screen.getByRole("button", { name: "Confirmar: desconectar Caixa" }));
    await waitFor(() => expect(calls).toEqual(["sync", "POST disconnect"]));
  });

  it("shows a pull that failed without leaking the aggregator's answer, and hides the writes from who cannot write", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") {
        return { status: 200, body: { ...profile, companies: [{ ...profile.companies[0], role: "viewer", permissions: { ...profile.companies[0].permissions, finance: "read" } }] } };
      }
      if (url.includes("/bank-accounts")) return { status: 200, body: ACCOUNTS };
      return { status: 200, body: { items: [connection({ last_error: "the aggregator is unavailable" })], available: true, provider: "pluggy" } };
    });
    renderAt("/companies/c1/bank-connections");

    expect(await screen.findByText("the aggregator is unavailable")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Sincronizar/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Desconectar/ })).not.toBeInTheDocument();
    expect(screen.queryByText("Conectar uma conta")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Histórico de Caixa" })).toBeInTheDocument();
  });
});

describe("the history of a connection", () => {
  it("names the bank account instead of showing its id", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/bank-accounts")) return { status: 200, body: ACCOUNTS };
      if (url.endsWith("/history")) {
        return {
          status: 200,
          body: [
            {
              action: "created",
              at: 1_800_000_000_000,
              actor: { kind: "user", id: "u1", name: "Ana" },
              changes: [
                { field: "bank_account_id", from: null, to: "a1" },
                { field: "status", from: null, to: "connected" },
              ],
            },
          ],
        };
      }
      return { status: 200, body: { items: [connection()], available: true, provider: "pluggy" } };
    });
    renderAt("/companies/c1/bank-connections");
    await userEvent.click(await screen.findByRole("button", { name: "Histórico de Caixa" }));

    expect((await screen.findByText(/Conta bancária:/)).closest("li")).toHaveTextContent("Conta bancária: Caixa");
  });
});
