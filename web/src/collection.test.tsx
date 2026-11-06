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

function serve(options: { provider?: string | null; account?: string | null; onSave?: (body: unknown) => Reply }) {
  return mockApi((url, init) => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
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
