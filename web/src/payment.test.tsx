import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { PublicCharge } from "./api";
import { mockApi, renderAt } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const base: PublicCharge = {
  company_name: "Padaria Pão Quente",
  description: "Venda 12",
  installment: 1,
  installments: 2,
  method: "pix",
  state: "payable",
  amount: "100",
  due_on: "2026-12-10",
  digitable_line: null,
  pix_payload: "00020126580014br.gov.bcb.pix",
  payment_url: null,
  paid_on: null,
};

function serve(charge: PublicCharge | null) {
  return mockApi((url) =>
    charge === null
      ? { status: 404, body: { error: "charge not found" } }
      : url === "/api/public/charges/tok"
        ? { status: 200, body: charge }
        : { status: 404, body: { error: "not found" } },
  );
}

it("shows what is charged and the Pix text to copy, with no session", async () => {
  const writeText = vi.fn(async () => undefined);
  vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });
  serve(base);
  renderAt("/pay/tok");

  expect(await screen.findByRole("heading", { name: "Pagamento para Padaria Pão Quente" })).toBeInTheDocument();
  expect(screen.getByText("Venda 12 — parcela 1 de 2")).toBeInTheDocument();
  expect(screen.getByText("R$ 100,00")).toBeInTheDocument();
  expect(screen.getByText(/vencimento 10\/12\/2026/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Copiar o código Pix" }));
  expect(writeText).toHaveBeenCalledWith("00020126580014br.gov.bcb.pix");
  expect(await screen.findByText("Copiado.")).toBeInTheDocument();
});

it("offers the boleto line and its paper, or the card link", async () => {
  serve({
    ...base,
    method: "boleto",
    pix_payload: null,
    digitable_line: "00190.00009 01234.567890",
    payment_url: "https://fake-provider.invalid/boleto/1",
  });
  const first = renderAt("/pay/tok");
  expect(await screen.findByText("00190.00009 01234.567890")).toBeInTheDocument();
  const paper = screen.getByRole("link", { name: "Abrir o boleto (PDF)" });
  expect(paper).toHaveAttribute("href", "https://fake-provider.invalid/boleto/1");
  expect(paper).toHaveAttribute("rel", "noopener noreferrer");
  first.unmount();

  serve({ ...base, method: "card_link", pix_payload: null, payment_url: "https://fake-provider.invalid/card/1" });
  renderAt("/pay/tok");
  expect(await screen.findByRole("link", { name: "Pagar com cartão" })).toBeInTheDocument();
});

it("says it was paid, or that there is nothing to pay, and never shows what to pay with", async () => {
  serve({ ...base, state: "paid", paid_on: "2026-12-09", pix_payload: null });
  const first = renderAt("/pay/tok");
  expect(await screen.findByText("Pagamento confirmado em 09/12/2026. Obrigado!")).toBeInTheDocument();
  expect(screen.queryByText("Pix copia e cola")).not.toBeInTheDocument();
  first.unmount();

  serve({ ...base, state: "unavailable", pix_payload: null });
  renderAt("/pay/tok");
  expect(await screen.findByText(/não está mais disponível para pagamento/)).toBeInTheDocument();
});

it("says the link leads nowhere", async () => {
  serve(null);
  renderAt("/pay/tok");
  expect(await screen.findByRole("alert")).toHaveTextContent("Este link não leva a nenhuma cobrança");
});
