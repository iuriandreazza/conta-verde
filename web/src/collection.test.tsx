import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import { mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const bank = (id: string, name: string, active = true) => ({
  id,
  company_id: "c1",
  kind: "checking",
  name,
  bank_code: null,
  branch: null,
  number: null,
  active,
});
const category = (id: string, name: string, dre_group = "operating_expenses", active = true) => ({
  id,
  company_id: "c1",
  parent_id: null,
  name,
  dre_group,
  cash_flow_group: "operating",
  level: 1,
  active,
});

function serve(options: {
  provider?: string | null;
  account?: string | null;
  onSave?: (body: unknown) => Reply;
  plan?: { enabled: boolean; rules: { days: number; channel: string }[]; whatsapp_available?: boolean };
  onPlan?: (body: unknown) => Reply;
}) {
  return mockApi((url, init) => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/collection/dunning") && init?.method === "PUT") {
      const body = JSON.parse(String(init.body));
      return options.onPlan?.(body) ?? { status: 200, body: { ...body, whatsapp_available: true } };
    }
    if (url.endsWith("/collection/dunning")) {
      return { status: 200, body: { whatsapp_available: true, ...(options.plan ?? { enabled: false, rules: [] }) } };
    }
    if (url.endsWith("/collection/settings") && init?.method === "PUT") {
      const body = JSON.parse(String(init.body));
      return options.onSave?.(body) ?? { status: 200, body };
    }
    if (url.endsWith("/collection/settings")) {
      return { status: 200, body: { bank_account_id: options.account ?? null, fee_category_id: null } };
    }
    if (url.endsWith("/collection")) {
      return { status: 200, body: { provider: options.provider === undefined ? "fake" : options.provider } };
    }
    if (url.endsWith("/categories")) {
      return {
        status: 200,
        body: {
          items: [
            category("k1", "Tarifas bancárias"),
            category("k2", "Transferências", "transfers"),
            category("k3", "Antiga", "operating_expenses", false),
          ],
        },
      };
    }
    if (url.includes("/bank-accounts")) {
      return { status: 200, body: { items: [bank("b1", "Conta Itaú"), bank("b2", "Conta antiga", false)], next_cursor: null } };
    }
    return { status: 200, body: { items: [], next_cursor: null } };
  });
}

it("saves the account charges are paid into and the category of the fee", async () => {
  const bodies: unknown[] = [];
  serve({
    onSave: (body) => {
      bodies.push(body);
      return { status: 200, body };
    },
  });
  renderAt("/companies/c1/collection");

  await userEvent.selectOptions(await screen.findByLabelText("Conta de recebimento"), "b1");
  const fees = screen.getByLabelText("Categoria da taxa do provedor");
  // Only what can be chosen: not an inactive category or the one for transfers.
  expect(Array.from((fees as HTMLSelectElement).options).map((option) => option.textContent)).toEqual([
    "Não lançar a taxa (fica só na cobrança)",
    "Tarifas bancárias",
  ]);
  await userEvent.selectOptions(fees, "k1");
  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));

  expect(await screen.findByText("Configuração da cobrança salva.")).toBeInTheDocument();
  expect(bodies).toEqual([{ bank_account_id: "b1", fee_category_id: "k1" }]);
});

it("shows the provider and the address it is told to call, or says none is set", async () => {
  serve({ account: "b1" });
  const first = renderAt("/companies/c1/collection");
  expect(await screen.findByText(/\/api\/public\/payments\/c1\/webhooks\/fake/)).toBeInTheDocument();
  expect(screen.getByLabelText("Conta de recebimento")).toHaveValue("b1");
  first.unmount();

  serve({ provider: null });
  renderAt("/companies/c1/collection");
  expect(await screen.findByText(/Nenhum provedor está configurado/)).toBeInTheDocument();
});

it("shows what the API refused", async () => {
  serve({ onSave: () => ({ status: 422, body: { error: "unusable bank account" } }) });
  renderAt("/companies/c1/collection");

  await userEvent.selectOptions(await screen.findByLabelText("Conta de recebimento"), "b1");
  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("unusable bank account");
});

it("saves the reminder plan as a whole: when, how many days and the channel", async () => {
  const plans: unknown[] = [];
  serve({
    plan: { enabled: false, rules: [{ days: -3, channel: "email" }] },
    onPlan: (body) => {
      plans.push(body);
      return { status: 200, body: { ...(body as object), whatsapp_available: true } };
    },
  });
  renderAt("/companies/c1/collection");

  await userEvent.click(await screen.findByRole("checkbox", { name: /Enviar lembretes/ }));
  await userEvent.click(screen.getByRole("button", { name: "Adicionar lembrete" }));
  await userEvent.selectOptions(screen.getByLabelText("Quando, lembrete 2"), "after");
  await userEvent.clear(screen.getByLabelText("Dias, lembrete 2"));
  await userEvent.type(screen.getByLabelText("Dias, lembrete 2"), "7");
  await userEvent.selectOptions(screen.getByLabelText("Canal, lembrete 2"), "whatsapp");
  await userEvent.click(screen.getByRole("button", { name: "Salvar lembretes" }));

  expect(await screen.findByText("Lembretes salvos.")).toBeInTheDocument();
  expect(plans).toEqual([
    {
      enabled: true,
      rules: [
        { days: -3, channel: "email" },
        { days: 7, channel: "whatsapp" },
      ],
    },
  ]);
});

it("refuses a rule without a number of days, offers no WhatsApp without a provider, and removes a rule", async () => {
  serve({ plan: { enabled: true, rules: [{ days: 0, channel: "email" }], whatsapp_available: false } });
  renderAt("/companies/c1/collection");

  expect(await screen.findByText(/WhatsApp não está configurado/)).toBeInTheDocument();
  const channel = screen.getByLabelText("Canal, lembrete 1") as HTMLSelectElement;
  expect(Array.from(channel.options).map((option) => option.textContent)).toEqual(["E-mail"]);

  await userEvent.click(screen.getByRole("button", { name: "Adicionar lembrete" }));
  await userEvent.clear(screen.getByLabelText("Dias, lembrete 2"));
  await userEvent.click(screen.getByRole("button", { name: "Salvar lembretes" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Informe quantos dias");

  await userEvent.click(screen.getByRole("button", { name: "Remover o lembrete 2" }));
  expect(screen.queryByLabelText("Dias, lembrete 2")).not.toBeInTheDocument();
});
