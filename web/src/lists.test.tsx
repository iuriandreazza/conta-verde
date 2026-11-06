import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Entry } from "./api";
import { mockApi, profile, renderAt } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

function entry(id: string, description: string, dueDate: string, settled: string): Entry {
  return {
    id,
    company_id: "c1",
    kind: "payable",
    description,
    party_id: null,
    category_id: "k1",
    cost_center_id: null,
    competence_date: "2000-01-01",
    installments: [{ number: 1, due_date: dueDate, amount: "100", settled }],
    total: "100",
    recurrence_id: null,
    sale_id: null,
    sale_return_id: null,
    voided: false,
  };
}

function serve(entries: Entry[], nextCursor: string | null) {
  mockApi((url) => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.startsWith("/api/companies/c1/entries")) return { status: 200, body: { items: entries, next_cursor: nextCursor } };
    return { status: 200, body: { items: [], next_cursor: null } };
  });
}

describe("how the entries list presents itself", () => {
  it("marks the overdue row with an edge and leaves the settled one alone", async () => {
    serve([entry("e1", "Aluguel atrasado", "2000-01-10", "0"), entry("e2", "Luz paga", "2000-01-10", "100")], null);
    renderAt("/companies/c1/entries");

    const late = (await screen.findByText("Aluguel atrasado")).closest("tr")!;
    const paid = screen.getByText("Luz paga").closest("tr")!;

    expect(late).toHaveClass("row-critical");
    expect(within(late).getByText("Vencido")).toBeInTheDocument();
    expect(paid).not.toHaveClass("row-critical");
  });

  it("counts the rows and says when the server has more", async () => {
    serve([entry("e1", "Aluguel", "2999-01-10", "0")], "next");
    renderAt("/companies/c1/entries");

    expect(await screen.findByText("Mostrando 1 lançamento, e há mais")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Carregar mais" })).toBeInTheDocument();
  });
});

describe("how the bank accounts list presents itself", () => {
  it("keeps the bank data under the name, in four columns, and counts the accounts", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/balances")) return { status: 200, body: { on: "2026-11-05", accounts: [], total: "0" } };
      if (url.startsWith("/api/companies/c1/bank-accounts")) {
        return {
          status: 200,
          body: {
            items: [
              { id: "a1", company_id: "c1", kind: "checking", name: "Conta principal", bank_code: "341", branch: "1234", number: "56789X", active: true },
            ],
            next_cursor: null,
          },
        };
      }
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/bank-accounts");

    const table = await screen.findByRole("table");
    expect(within(table).getAllByRole("columnheader")).toHaveLength(4);
    expect(within(table).getByText("Conta corrente · 341 Itaú · ag. 1234 · conta 56789X")).toBeInTheDocument();
    expect(await screen.findByText("Mostrando 1 conta")).toBeInTheDocument();
  });
});

describe("creating from the top bar", () => {
  it("raises the sheet over the page that is open, without leaving it", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/parties");
    await screen.findByRole("heading", { name: "Clientes e fornecedores" });

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Novo lançamento" }));

    expect(await screen.findByRole("dialog", { name: "Novo lançamento" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Clientes e fornecedores" })).toBeInTheDocument();
  });

  it("says the entry was added and the list on the page reads again", async () => {
    const category = { id: "k1", company_id: "c1", parent_id: null, name: "Aluguel", dre_group: "operating_expenses", cash_flow_group: "operating", level: 1, active: true };
    const created = entry("e9", "Aluguel de novembro", "2999-01-10", "0");
    let lists = 0;
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/categories")) return { status: 200, body: { items: [category] } };
      if (init?.method === "POST" && url.endsWith("/entries")) return { status: 201, body: created };
      if (url.startsWith("/api/companies/c1/entries")) {
        lists += 1;
        return { status: 200, body: { items: lists > 1 ? [created] : [], next_cursor: null } };
      }
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/entries");

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Novo lançamento" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Novo lançamento" }));
    await userEvent.type(await dialog.findByLabelText("Descrição"), "Aluguel de novembro");
    await userEvent.selectOptions(dialog.getByLabelText("Categoria"), "k1");
    fireEvent.change(dialog.getByLabelText("Competência"), { target: { value: "2999-01-01" } });
    await userEvent.type(dialog.getByLabelText("Valor total"), "100");
    fireEvent.change(dialog.getByLabelText("Primeiro vencimento"), { target: { value: "2999-01-10" } });
    await userEvent.click(dialog.getByRole("button", { name: "Gerar parcelas" }));
    await userEvent.click(dialog.getByRole("button", { name: /^Adicionar$/ }));

    expect(await screen.findByText("Aluguel de novembro adicionado.")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(await screen.findByText("Aluguel de novembro", { selector: ".cell-title" })).toBeInTheDocument();
  });
});
