import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { WebhookDelivery, WebhookSubscription } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt } from "./testSupport";

const subscription = (overrides: Partial<WebhookSubscription> = {}): WebhookSubscription => ({
  id: "w1",
  url: "https://loja.example.com/hooks",
  events: ["sale.completed", "charge.paid"],
  active: true,
  created_by: "Ana",
  created_at: 1_800_000_000_000,
  ...overrides,
});

const delivery = (overrides: Partial<WebhookDelivery> = {}): WebhookDelivery => ({
  id: "d1",
  event_id: "e1",
  kind: "sale.completed",
  status: "delivered",
  attempts: 1,
  last_status: 200,
  last_error: null,
  created_at: 1_800_000_000_000,
  updated_at: 1_800_000_000_000,
  delivered_at: 1_800_000_000_000,
  ...overrides,
});

const SECRET = "whsec_" + "b".repeat(64);
const EVENTS = ["sale.created", "sale.completed", "sale.cancelled", "charge.paid", "stock.below_minimum"];

beforeEach(() => {
  vi.restoreAllMocks();
  Object.defineProperty(navigator, "clipboard", { value: { writeText: vi.fn(async () => undefined) }, configurable: true });
});
afterEach(() => vi.unstubAllGlobals());

describe("the webhooks page", () => {
  it("lists the destinations with what each listens to and says when webhooks are off", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      return {
        status: 200,
        body: { items: [subscription(), subscription({ id: "w2", url: "https://b.example.com/x", events: [], active: false })], available: false, events: EVENTS },
      };
    });
    renderAt("/companies/c1/webhooks");

    expect(await screen.findByText("https://loja.example.com/hooks")).toBeInTheDocument();
    expect(screen.getByText("Venda concluída, Cobrança paga")).toBeInTheDocument();
    expect(screen.getByText("Todos os eventos")).toBeInTheDocument();
    expect(screen.getByText("Ativo")).toBeInTheDocument();
    expect(screen.getByText("Desligado")).toBeInTheDocument();
    expect(screen.getByText(/Webhooks desligados: o servidor não tem a chave/)).toBeInTheDocument();
    // With no key nothing can be made.
    expect(screen.queryByRole("button", { name: "Novo webhook" })).not.toBeInTheDocument();
  });

  it("makes a webhook from the events chosen and shows the secret once", async () => {
    let created: { url: string; events: string[] } | null = null;
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url === "/api/companies/c1/webhook-subscriptions" && init?.method === "POST") {
        created = JSON.parse(String(init.body));
        return { status: 201, body: { subscription: subscription({ events: ["sale.created"] }), secret: SECRET } };
      }
      return { status: 200, body: { items: [], available: true, events: EVENTS } };
    });
    renderAt("/companies/c1/webhooks");
    await screen.findByText("Nenhum webhook ainda.");

    await userEvent.click(screen.getByRole("button", { name: "Novo webhook" }));
    const sheet = await screen.findByRole("dialog");
    await userEvent.type(within(sheet).getByLabelText("Endereço (https)"), "https://loja.example.com/hooks");
    await userEvent.click(within(sheet).getByLabelText("Pedido criado"));
    await userEvent.click(within(sheet).getByRole("button", { name: "Criar webhook" }));

    expect(await screen.findByDisplayValue(SECRET)).toBeInTheDocument();
    expect(created).toEqual({ url: "https://loja.example.com/hooks", events: ["sale.created"] });
    expect(screen.getByText(/ele não aparece de novo/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Copiar o segredo de assinatura" }));
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(SECRET);
  });

  it("sends a test event and reads the log, where a delivery can be sent again", async () => {
    const calls: string[] = [];
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/test")) {
        calls.push("test");
        return { status: 202, body: delivery({ id: "d9", kind: "webhook.test", status: "pending", attempts: 0 }) };
      }
      if (url.endsWith("/redeliver")) {
        calls.push(`${init?.method} ${url}`);
        return { status: 202, body: delivery({ status: "pending", attempts: 0 }) };
      }
      if (url.includes("/deliveries")) {
        return {
          status: 200,
          body: {
            items: [
              delivery(),
              delivery({ id: "d2", kind: "charge.paid", status: "pending", attempts: 2, last_status: 503, last_error: "the subscriber answered 503" }),
              delivery({ id: "d3", kind: "sale.created", status: "failed", attempts: 15, last_status: null, last_error: "timed out" }),
            ],
            next_cursor: null,
          },
        };
      }
      return { status: 200, body: { items: [subscription()], available: true, events: EVENTS } };
    });
    renderAt("/companies/c1/webhooks");
    await userEvent.click(await screen.findByRole("button", { name: "Testar https://loja.example.com/hooks" }));
    expect(await screen.findByText(/Evento de teste na fila/)).toBeInTheDocument();
    expect(calls).toEqual(["test"]);

    await userEvent.click(screen.getByRole("button", { name: "Log de https://loja.example.com/hooks" }));
    expect(await screen.findByText("Entregue")).toBeInTheDocument();
    expect(screen.getByText("Aguardando")).toBeInTheDocument();
    expect(screen.getByText("Desistiu")).toBeInTheDocument();
    expect(screen.getByText("the subscriber answered 503")).toBeInTheDocument();
    expect(screen.getByText("timed out")).toBeInTheDocument();
    expect(screen.getByText(/nunca o que o destino respondeu/)).toBeInTheDocument();

    const failed = screen.getAllByRole("button", { name: /Enviar de novo: Pedido criado/ })[0];
    await userEvent.click(failed);
    await waitFor(() => expect(calls).toContain("POST /api/companies/c1/webhook-subscriptions/w1/deliveries/d3/redeliver"));
  });

  it("replaces the secret with one of the person's own, or with one made here and shown once", async () => {
    const bodies: unknown[] = [];
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/secret")) {
        const body = JSON.parse(String(init?.body));
        bodies.push(body);
        return { status: 200, body: { secret: body.secret === undefined ? SECRET : null } };
      }
      return { status: 200, body: { items: [subscription()], available: true, events: EVENTS } };
    });
    renderAt("/companies/c1/webhooks");
    await userEvent.click(await screen.findByRole("button", { name: "Segredo de https://loja.example.com/hooks" }));

    await userEvent.type(screen.getByLabelText("Ou use o seu"), "my-own-secret-123");
    await userEvent.click(screen.getByRole("button", { name: "Usar este segredo" }));
    expect(await screen.findByText(/O anterior continua assinando por 24 horas/)).toBeInTheDocument();
    expect(bodies[0]).toEqual({ secret: "my-own-secret-123" });
    // What was typed is not shown again, nor kept in the field.
    expect(screen.queryByDisplayValue("my-own-secret-123")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Gerar um novo segredo" }));
    expect(await screen.findByDisplayValue(SECRET)).toBeInTheDocument();
    expect(bodies[1]).toEqual({});
  });

  it("edits where, what and whether, sending the whole subscription", async () => {
    let put: unknown = null;
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") {
        put = JSON.parse(String(init.body));
        return { status: 200, body: subscription({ active: false }) };
      }
      return { status: 200, body: { items: [subscription()], available: true, events: EVENTS } };
    });
    renderAt("/companies/c1/webhooks");
    await userEvent.click(await screen.findByRole("button", { name: "Editar https://loja.example.com/hooks" }));
    await userEvent.click(screen.getByLabelText("Ativo"));
    await userEvent.click(screen.getByLabelText("Pedido criado"));
    await userEvent.click(screen.getByRole("button", { name: "Salvar" }));

    await waitFor(() =>
      expect(put).toEqual({ url: "https://loja.example.com/hooks", events: ["sale.completed", "charge.paid", "sale.created"], active: false }),
    );
  });

  it("shows only the log and the trail to someone who may only read", async () => {
    const reader = { ...profile, companies: [{ ...profile.companies[0], permissions: { ...OWNER_PERMISSIONS, members: "read" as const } }] };
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: reader };
      return { status: 200, body: { items: [subscription()], available: true, events: EVENTS } };
    });
    renderAt("/companies/c1/webhooks");
    await screen.findByText("https://loja.example.com/hooks");
    expect(screen.queryByRole("button", { name: "Novo webhook" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Testar/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Segredo de/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Editar/ })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Log de/ })).toBeInTheDocument();
  });
});
