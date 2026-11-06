import { fireEvent, screen, waitFor, within } from "@testing-library/react";
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
    purchase_receipt_id: null,
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
    await userEvent.click(screen.getByRole("menuitem", { name: "Nova receita" }));

    expect(await screen.findByRole("dialog", { name: "Nova receita" })).toBeInTheDocument();
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
    await userEvent.click(screen.getByRole("menuitem", { name: "Nova receita" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Nova receita" }));
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

  it("does not close the sheet under a save that is still on its way", async () => {
    const category = { id: "k1", company_id: "c1", parent_id: null, name: "Aluguel", dre_group: "operating_expenses", cash_flow_group: "operating", level: 1, active: true };
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/categories")) return { status: 200, body: { items: [category] } };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    const answer = fetchMock.getMockImplementation()!;
    // The save never answers: it stays in flight.
    fetchMock.mockImplementation((input, init) =>
      String(input).endsWith("/entries") && init?.method === "POST" ? new Promise<Response>(() => undefined) : answer(input, init),
    );
    renderAt("/companies/c1/entries");
    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Nova receita" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Nova receita" }));
    await userEvent.type(await dialog.findByLabelText("Descrição"), "Aluguel");
    await userEvent.selectOptions(dialog.getByLabelText("Categoria"), "k1");
    fireEvent.change(dialog.getByLabelText("Competência"), { target: { value: "2999-01-01" } });
    await userEvent.type(dialog.getByLabelText("Valor total"), "100");
    fireEvent.change(dialog.getByLabelText("Primeiro vencimento"), { target: { value: "2999-01-10" } });
    await userEvent.click(dialog.getByRole("button", { name: "Gerar parcelas" }));
    await userEvent.click(dialog.getByRole("button", { name: /^Adicionar$/ }));

    await userEvent.keyboard("{Escape}");
    await userEvent.click(dialog.getByRole("button", { name: "Fechar" }));

    expect(screen.getByRole("dialog", { name: "Nova receita" })).toBeInTheDocument();
  });

  it("registers a cost center from the top bar over another page, and says so", async () => {
    const created = { id: "cc1", company_id: "c1", name: "Loja Norte", active: true };
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST" && url.endsWith("/cost-centers")) return { status: 201, body: created };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/parties");
    await screen.findByRole("heading", { name: "Clientes e fornecedores" });

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Novo centro de custo" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Novo centro de custo" }));
    await userEvent.type(await dialog.findByLabelText("Nome"), "Loja Norte");
    await userEvent.click(dialog.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByText("Loja Norte cadastrado.")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Clientes e fornecedores" })).toBeInTheDocument();
  });

  it("registers a payment method from the top bar with its kind", async () => {
    const created = { id: "pm1", company_id: "c1", kind: "bank_slip", name: "Boleto", active: true };
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST" && url.endsWith("/payment-methods")) return { status: 201, body: created };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/parties");

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Nova forma de pagamento" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Nova forma de pagamento" }));
    await userEvent.selectOptions(await dialog.findByLabelText("Tipo"), "bank_slip");
    await userEvent.type(dialog.getByLabelText("Nome"), "Boleto");
    await userEvent.click(dialog.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByText("Boleto cadastrada.")).toBeInTheDocument();
    const post = fetchMock.mock.calls.find(([, init]) => init?.method === "POST")!;
    expect(JSON.parse(String(post[1]?.body))).toEqual({ kind: "bank_slip", name: "Boleto", active: true });
  });

  it("opens the payment term sheet from the top bar with a way back", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Novo prazo de pagamento" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Novo prazo de pagamento" }));
    await userEvent.click(await dialog.findByRole("button", { name: "Voltar" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("offers no new-record button to a role that only reads", async () => {
    const reader = { ...profile, companies: [{ ...profile.companies[0], permissions: { finance: "read" as const } }] };
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: reader } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/cost-centers");

    await screen.findByRole("heading", { name: "Centros de custo" });
    expect(screen.queryByRole("button", { name: "Novo centro de custo" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Novo registro" })).not.toBeInTheDocument();
  });

  it("registers a bank account from the top bar and reads the balances again", async () => {
    const created = { id: "a1", company_id: "c1", kind: "cash", name: "Caixa da loja", bank_code: null, branch: null, number: null, active: true };
    let balanceReads = 0;
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/balances")) {
        balanceReads += 1;
        return { status: 200, body: { on: "2026-11-05", accounts: [], total: "0" } };
      }
      if (init?.method === "POST" && url.endsWith("/bank-accounts")) return { status: 201, body: created };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/bank-accounts");
    await screen.findByRole("heading", { name: "Contas bancárias" });
    const before = balanceReads;

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Nova conta bancária" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Nova conta bancária" }));
    await userEvent.selectOptions(await dialog.findByLabelText("Tipo"), "cash");
    await userEvent.type(dialog.getByLabelText("Nome"), "Caixa da loja");
    await userEvent.click(dialog.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByText("Caixa da loja cadastrada.")).toBeInTheDocument();
    await waitFor(() => expect(balanceReads).toBeGreaterThan(before));
  });

  it("invites from the top bar and keeps the link on screen until the person is done", async () => {
    const invited = { id: "i1", email: "bia@example.com", role: "viewer", expires_at: Date.UTC(2026, 11, 1), link: "https://erp.example/invitations/abc", emailed: true };
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST" && url.endsWith("/invitations")) return { status: 201, body: invited };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/parties");

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Convidar alguém" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Convidar alguém" }));
    await userEvent.type(await dialog.findByLabelText("E-mail"), "bia@example.com");
    await userEvent.click(dialog.getByRole("button", { name: "Convidar" }));

    // The link is shown once: the sheet stays on it, and no confirmation takes its place.
    expect(await dialog.findByLabelText("Link do convite")).toHaveValue("https://erp.example/invitations/abc");
    // A stray key does not throw the link away; only "Concluir" (or "Fechar") does.
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: "Convidar alguém" })).toBeInTheDocument();
    await userEvent.click(dialog.getByRole("button", { name: "Concluir" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("registers a recurrence from the top bar and says what was generated", async () => {
    const category = { id: "k1", company_id: "c1", parent_id: null, name: "Aluguel", dre_group: "operating_expenses", cash_flow_group: "operating", level: 1, active: true };
    const created = { id: "r1", company_id: "c1", kind: "payable", description: "Aluguel mensal", party_id: null, category_id: "k1", cost_center_id: null, amount: "2500", frequency: "monthly", starts_on: "2999-01-10", ends_on: null, active: true };
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/categories")) return { status: 200, body: { items: [category] } };
      if (init?.method === "POST" && url.endsWith("/recurrences")) return { status: 201, body: created };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/parties");

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Nova recorrência" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Nova recorrência" }));
    await userEvent.type(await dialog.findByLabelText("Descrição"), "Aluguel mensal");
    await userEvent.selectOptions(dialog.getByLabelText("Categoria"), "k1");
    await userEvent.type(dialog.getByLabelText("Valor de cada vez"), "2500");
    fireEvent.change(dialog.getByLabelText("Primeiro vencimento"), { target: { value: "2999-01-10" } });
    await userEvent.click(dialog.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByText(/Aluguel mensal cadastrada: os lançamentos que vencem/)).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("offers no invite button to a role that only reads the members", async () => {
    const reader = { ...profile, companies: [{ ...profile.companies[0], permissions: { members: "read" as const } }] };
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: reader } : { status: 200, body: [] },
    );
    renderAt("/companies/c1/members");

    await screen.findByRole("heading", { name: "Membros" });
    expect(screen.queryByRole("button", { name: "Convidar alguém" })).not.toBeInTheDocument();
  });

  it("starts a record from anywhere with Alt and its letter, once, and not under an open sheet", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");
    await screen.findByRole("heading", { name: "Clientes e fornecedores" });
    await screen.findByRole("button", { name: "Novo registro" });

    await userEvent.keyboard("{Alt>}{r}{/Alt}");
    expect(await screen.findByRole("dialog", { name: "Nova receita" })).toBeInTheDocument();

    // A sheet is open: another shortcut does not swap it for another form, which would lose what was typed.
    fireEvent.keyDown(window, { key: "t", code: "KeyT", altKey: true });
    expect(screen.getByRole("dialog", { name: "Nova receita" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Nova transferência" })).not.toBeInTheDocument();
  });

  it("shows the letter beside each item that has one, without making it part of the name", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));

    const item = screen.getByRole("menuitem", { name: "Nova receita" });
    expect(item).toHaveAttribute("aria-keyshortcuts", "Alt+R");
    expect(item).toHaveTextContent("Alt+R");
  });

  it("closes the menu when a shortcut is used while it is open", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");
    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "r", code: "KeyR", altKey: true });

    expect(await screen.findByRole("dialog", { name: "Nova receita" })).toBeInTheDocument();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("ignores a key held down", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");
    await screen.findByRole("button", { name: "Novo registro" });

    fireEvent.keyDown(window, { key: "r", code: "KeyR", altKey: true, repeat: true });

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it.each([
    ["Nova tabela de preço", "Nome"],
    ["Nova regra de comissão", "Percentual (%)"],
    ["Novo contrato", "Ciclo de cobrança"],
    ["Novo orçamento", "Emitido em"],
    ["Novo pedido", "Data da venda"],
  ])("raises the %s sheet from the top bar, with its fields once what it points at is read", async (item, field) => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    await userEvent.click(screen.getByRole("menuitem", { name: item }));
    const dialog = within(await screen.findByRole("dialog", { name: item }));

    expect(await dialog.findByLabelText(field)).toBeInTheDocument();
    await userEvent.click(dialog.getByRole("button", { name: "Voltar" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens a new expense with Alt+D, then a revenue, each with its own title and kind", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");
    await screen.findByRole("button", { name: "Novo registro" });

    fireEvent.keyDown(window, { key: "d", code: "KeyD", altKey: true });
    const expense = within(await screen.findByRole("dialog", { name: "Nova despesa" }));
    expect(await expense.findByLabelText("Tipo")).toHaveValue("payable");
    await userEvent.click(expense.getByRole("button", { name: "Voltar" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    fireEvent.keyDown(window, { key: "r", code: "KeyR", altKey: true });
    const revenue = within(await screen.findByRole("dialog", { name: "Nova receita" }));
    expect(await revenue.findByLabelText("Tipo")).toHaveValue("receivable");
  });

  it("starts an expense as a payable, and titles the sheet for it", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Nova despesa" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Nova despesa" }));

    expect(await dialog.findByLabelText("Tipo")).toHaveValue("payable");
  });
});

describe("contas a receber and contas a pagar", () => {
  it("are the entries list with the kind in the address: titled, filtered and offered as such", async () => {
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/entries?tipo=payable");

    expect(await screen.findByRole("heading", { name: "Contas a pagar" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Nova despesa" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Contas a pagar" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "Lançamentos" })).not.toHaveAttribute("aria-current");
    const lists = fetchMock.mock.calls.map(([url]) => String(url)).filter((url) => url.includes("/entries"));
    expect(lists.some((url) => url.includes("kind=payable"))).toBe(true);
  });

  it("move between each other from the menu without leaving the page", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/entries");
    expect(await screen.findByRole("heading", { name: "Lançamentos" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("link", { name: "Contas a receber" }));

    expect(await screen.findByRole("heading", { name: "Contas a receber" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Contas a receber" })).toHaveAttribute("aria-current", "page");
  });

  it("asks for the list of its kind and nothing else, and its button starts that kind", async () => {
    const fetchMock = mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/entries?tipo=payable");
    await userEvent.click(await screen.findByRole("button", { name: "Nova despesa" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Nova despesa" }));

    expect(await dialog.findByLabelText("Tipo")).toHaveValue("payable");
    const lists = fetchMock.mock.calls.map(([url]) => String(url)).filter((url) => /\/entries\?/.test(url));
    expect(lists.length).toBeGreaterThan(0);
    expect(lists.every((url) => url.includes("kind=payable"))).toBe(true);
  });

  it("writes the kind into the address when 'Mostrar' changes, and the menu follows", async () => {
    const fetchMock = mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/entries");
    await screen.findByRole("heading", { name: "Lançamentos" });
    expect(screen.getByRole("link", { name: "Lançamentos" })).toHaveAttribute("aria-current", "page");

    await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "receivable");

    expect(await screen.findByRole("heading", { name: "Contas a receber" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Contas a receber" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "Lançamentos" })).not.toHaveAttribute("aria-current");
    await waitFor(() =>
      expect(fetchMock.mock.calls.some(([url]) => /\/entries\?/.test(String(url)) && String(url).includes("kind=receivable"))).toBe(true),
    );
  });

  it("reads an unknown kind in the address as every kind, in the title and in the menu", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/entries?tipo=foo");

    expect(await screen.findByRole("heading", { name: "Lançamentos" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Lançamentos" })).toHaveAttribute("aria-current", "page");
  });
});
