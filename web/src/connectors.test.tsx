import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Connector, ConnectorList } from "./api";
import { mockApi, profile, renderAt } from "./testSupport";

const connector = (overrides: Partial<Connector> = {}): Connector => ({
  id: "k1",
  kind: "woocommerce",
  name: "Loja do centro",
  active: false,
  state: "off",
  key_usable: false,
  subscription_on: false,
  api_key_id: null,
  webhook_subscription_id: null,
  created_at: 1_800_000_000_000,
  created_by: "Ana",
  ...overrides,
});

const CATALOGUE = [{ kind: "woocommerce", title: "WooCommerce", scopes: ["products:read", "sales:write"], events: ["sale.completed", "sale.cancelled"], package_path: "connectors/woocommerce" }];
const listOf = (items: Connector[], available = true): ConnectorList => ({ items, catalogue: CATALOGUE, available });

beforeEach(() => {
  vi.restoreAllMocks();
  Object.defineProperty(navigator, "clipboard", { value: { writeText: vi.fn(async () => undefined) }, configurable: true });
});
afterEach(() => vi.unstubAllGlobals());

describe("the connectors page", () => {
  it("lists the connectors with their state and says why one that is on needs the person, and says when they are off", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      return {
        status: 200,
        body: listOf(
          [
            connector({ active: true, state: "on", key_usable: true, subscription_on: true }),
            connector({ id: "k2", name: "Loja do norte", active: true, state: "attention", key_usable: false, subscription_on: true }),
            connector({ id: "k3", name: "Loja do sul" }),
          ],
          false,
        ),
      };
    });
    renderAt("/companies/c1/connectors");

    expect(await screen.findByText("Loja do centro")).toBeInTheDocument();
    expect(screen.getByText("Ligado")).toBeInTheDocument();
    expect(screen.getByText("Precisa de atenção")).toBeInTheDocument();
    expect(screen.getByText(/A chave foi revogada ou expirou em Chaves de API/)).toBeInTheDocument();
    expect(screen.getByText("Desligado")).toBeInTheDocument();
    expect(screen.getByText(/Conectores desligados: o servidor não tem a chave/)).toBeInTheDocument();
    expect(screen.queryByText("Novo conector")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ligar Loja do sul" })).toBeDisabled();
  });

  it("makes a connector, turns it on at an address and shows the key and the secret once", async () => {
    const sent: unknown[] = [];
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url === "/api/companies/c1/connectors" && init?.method === "POST") {
        sent.push(JSON.parse(String(init.body)));
        return { status: 201, body: connector() };
      }
      if (url === "/api/companies/c1/connectors/k1/activate") {
        sent.push(JSON.parse(String(init?.body)));
        return {
          status: 200,
          body: { connector: connector({ active: true, state: "on", key_usable: true, subscription_on: true }), api_key_secret: "cvk_" + "a".repeat(64), webhook_secret: "whsec_" + "b".repeat(64) },
        };
      }
      return { status: 200, body: listOf([connector()]) };
    });
    renderAt("/companies/c1/connectors");
    await userEvent.type(await screen.findByLabelText("Nome da loja"), "Loja do centro");
    await userEvent.click(screen.getByRole("button", { name: "Criar" }));
    await waitFor(() => expect(sent[0]).toEqual({ kind: "woocommerce", name: "Loja do centro" }));

    await userEvent.click(await screen.findByRole("button", { name: "Ligar Loja do centro" }));
    await userEvent.type(screen.getByLabelText(/Endereço do pacote/), "https://conector.example.com/webhooks/conta-verde");
    await userEvent.click(screen.getByRole("button", { name: "Ligar" }));

    expect(await screen.findByDisplayValue("cvk_" + "a".repeat(64))).toBeInTheDocument();
    expect(screen.getByDisplayValue("whsec_" + "b".repeat(64))).toBeInTheDocument();
    expect(sent[1]).toEqual({ address: "https://conector.example.com/webhooks/conta-verde" });
    expect(screen.getByText(/não aparecem de novo/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "README do pacote" })).toHaveAttribute("href", expect.stringContaining("connectors/woocommerce"));
    await userEvent.click(screen.getByRole("button", { name: "Já copiei" }));
    expect(screen.queryByDisplayValue("cvk_" + "a".repeat(64))).not.toBeInTheDocument();
  });

  it("turns a connector off and hides the writes from who cannot write", async () => {
    const calls: string[] = [];
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/deactivate")) {
        calls.push(`${init?.method} ${url}`);
        return { status: 200, body: connector() };
      }
      return { status: 200, body: listOf([connector({ active: true, state: "on", key_usable: true, subscription_on: true })]) };
    });
    renderAt("/companies/c1/connectors");
    await userEvent.click(await screen.findByRole("button", { name: "Desligar Loja do centro" }));
    await waitFor(() => expect(calls).toEqual(["POST /api/companies/c1/connectors/k1/deactivate"]));
    expect(await screen.findByText(/a chave foi revogada e o webhook, desligado/)).toBeInTheDocument();
  });

  it("shows no write to someone who may only read", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") {
        return { status: 200, body: { ...profile, companies: [{ ...profile.companies[0], role: "viewer", permissions: { ...profile.companies[0].permissions, members: "read" } }] } };
      }
      return { status: 200, body: listOf([connector({ active: true, state: "on", key_usable: true, subscription_on: true })]) };
    });
    renderAt("/companies/c1/connectors");
    expect(await screen.findByText("Loja do centro")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Desligar/ })).not.toBeInTheDocument();
    expect(screen.queryByText("Novo conector")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Histórico de Loja do centro" })).toBeInTheDocument();
  });
});
