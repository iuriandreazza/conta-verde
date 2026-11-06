import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Charge, Entry } from "./api";
import { mockApi, profile, renderAt, type Reply } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const sale: Entry = {
  id: "e1",
  company_id: "c1",
  kind: "receivable",
  description: "Venda parcelada",
  party_id: "p1",
  category_id: "cat1",
  cost_center_id: null,
  competence_date: "2026-11-05",
  installments: [
    { number: 1, due_date: "2026-12-10", amount: "100", settled: "40" },
    { number: 2, due_date: "2027-01-10", amount: "50.5", settled: "0" },
  ],
  total: "150.5",
  recurrence_id: null,
  sale_id: null,
  sale_return_id: null,
  voided: false,
};

const pixCharge: Charge = {
  id: "ch1",
  entry_id: "e1",
  installment: 1,
  method: "pix",
  status: "open",
  amount: "60",
  due_on: "2026-12-10",
  provider: "fake",
  digitable_line: null,
  pix_payload: "00020126580014br.gov.bcb.pix",
  payment_url: null,
  issued_by: "Ana",
};

function serve(options: {
  provider?: string | null;
  charges: Charge[];
  onIssue?: (body: unknown) => Reply;
  onCancel?: () => Reply;
}) {
  const charges = [...options.charges];
  return mockApi((url, init) => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/collection")) {
      return { status: 200, body: { provider: options.provider === undefined ? "fake" : options.provider } };
    }
    if (url.endsWith("/entries/e1/charges") && init?.method === "POST") {
      const reply = options.onIssue?.(JSON.parse(String(init.body)));
      if (reply && reply.status === 201) {
        charges.push(reply.body as Charge);
      }
      return reply ?? { status: 500 };
    }
    if (url.endsWith("/entries/e1/charges")) return { status: 200, body: { items: charges } };
    if (url.endsWith("/cancel")) {
      const reply = options.onCancel?.() ?? { status: 500 };
      if (reply.status === 200) charges[0] = reply.body as Charge;
      return reply;
    }
    if (url.includes("/entries")) return { status: 200, body: { items: [sale], next_cursor: null } };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [] } };
    return { status: 200, body: { items: [], next_cursor: null } };
  });
}

async function openPanel() {
  renderAt("/companies/c1/entries");
  await userEvent.click(await screen.findByRole("button", { name: "Cobranças de Venda parcelada" }));
  return within(await screen.findByRole("region", { name: "Cobranças de Venda parcelada" }));
}

it("shows each charge with what the customer pays with, and what is left to charge", async () => {
  serve({ charges: [pixCharge] });
  const panel = await openPanel();

  const item = await panel.findByRole("listitem");
  expect(item).toHaveTextContent("Parcela 1 · Pix · R$ 60,00, vence em 10/12/2026");
  expect(item).toHaveTextContent("Aberta");
  expect(item).toHaveTextContent("00020126580014br.gov.bcb.pix");
  // The installment that has an open charge is not offered again; the other is, with its balance.
  const options = within(panel.getByLabelText("Parcela")).getAllByRole("option");
  expect(options).toHaveLength(1);
  expect(options[0]).toHaveTextContent("2 — vence em 10/01/2027, saldo R$ 50,50");
});

it("issues a charge for the chosen installment and method, on another day when asked", async () => {
  const issued: Charge = {
    ...pixCharge,
    id: "ch2",
    installment: 2,
    method: "boleto",
    amount: "50.5",
    due_on: "2027-01-15",
    pix_payload: null,
    digitable_line: "00190.00009 01234.567890",
    payment_url: "https://fake-provider.invalid/boleto/1",
  };
  const bodies: unknown[] = [];
  serve({
    charges: [pixCharge],
    onIssue: (body) => {
      bodies.push(body);
      return { status: 201, body: issued };
    },
  });
  const panel = await openPanel();

  await userEvent.selectOptions(await panel.findByLabelText("Forma"), "boleto");
  await userEvent.type(panel.getByLabelText("Vencimento da cobrança"), "2027-01-15");
  await userEvent.click(panel.getByRole("button", { name: "Emitir cobrança" }));

  expect(await panel.findByText("Cobrança emitida para a parcela 2.")).toBeInTheDocument();
  expect(bodies).toEqual([{ installment: 2, method: "boleto", due_on: "2027-01-15" }]);
  const boleto = (await panel.findAllByRole("listitem")).find((item) => item.textContent?.includes("Boleto"))!;
  expect(boleto).toHaveTextContent("00190.00009 01234.567890");
  expect(within(boleto).getByRole("link", { name: "Ver o boleto" })).toHaveAttribute(
    "href",
    "https://fake-provider.invalid/boleto/1",
  );
  expect(within(boleto).getByRole("link", { name: "Ver o boleto" })).toHaveAttribute("rel", "noopener noreferrer");
});

it("says why the provider refused, and cancels an open charge", async () => {
  serve({
    charges: [pixCharge],
    onIssue: () => ({ status: 422, body: { error: "the payment provider refused the charge" } }),
    onCancel: () => ({ status: 200, body: { ...pixCharge, status: "cancelled" } }),
  });
  const panel = await openPanel();

  await userEvent.click(await panel.findByRole("button", { name: "Emitir cobrança" }));
  expect(await panel.findByRole("alert")).toHaveTextContent("the payment provider refused the charge");

  await userEvent.click(panel.getByRole("button", { name: "Cancelar a cobrança da parcela 1" }));
  await waitFor(() => expect(panel.getByRole("listitem")).toHaveTextContent("Cancelada"));
  expect(panel.queryByRole("button", { name: /Cancelar a cobrança/ })).not.toBeInTheDocument();
});

it("says so, and offers no form, when the operator configured no provider", async () => {
  serve({ provider: null, charges: [] });
  const panel = await openPanel();

  expect(await panel.findByText(/não está configurada nesta instalação/)).toBeInTheDocument();
  expect(panel.queryByRole("button", { name: "Emitir cobrança" })).not.toBeInTheDocument();
});

it("copies the Pix text", async () => {
  const writeText = vi.fn(async () => undefined);
  vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });
  serve({ charges: [pixCharge] });
  const panel = await openPanel();

  await userEvent.click(await panel.findByRole("button", { name: "Copiar o código Pix" }));
  expect(writeText).toHaveBeenCalledWith("00020126580014br.gov.bcb.pix");
  expect(await panel.findByText("Copiado.")).toBeInTheDocument();
  expect(panel.getByText(/Provedor de teste/)).toBeInTheDocument();
});
