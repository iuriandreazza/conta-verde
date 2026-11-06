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
    await userEvent.click(screen.getByRole("menuitem", { name: "Novo lançamento" }));
    const dialog = within(await screen.findByRole("dialog", { name: "Novo lançamento" }));
    await userEvent.type(await dialog.findByLabelText("Descrição"), "Aluguel");
    await userEvent.selectOptions(dialog.getByLabelText("Categoria"), "k1");
    fireEvent.change(dialog.getByLabelText("Competência"), { target: { value: "2999-01-01" } });
    await userEvent.type(dialog.getByLabelText("Valor total"), "100");
    fireEvent.change(dialog.getByLabelText("Primeiro vencimento"), { target: { value: "2999-01-10" } });
    await userEvent.click(dialog.getByRole("button", { name: "Gerar parcelas" }));
    await userEvent.click(dialog.getByRole("button", { name: /^Adicionar$/ }));

    await userEvent.keyboard("{Escape}");
    await userEvent.click(dialog.getByRole("button", { name: "Fechar" }));

    expect(screen.getByRole("dialog", { name: "Novo lançamento" })).toBeInTheDocument();
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

    await userEvent.keyboard("{Alt>}{l}{/Alt}");
    expect(await screen.findByRole("dialog", { name: "Novo lançamento" })).toBeInTheDocument();

    // A sheet is open: another shortcut does not swap it for another form, which would lose what was typed.
    fireEvent.keyDown(window, { key: "t", code: "KeyT", altKey: true });
    expect(screen.getByRole("dialog", { name: "Novo lançamento" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Nova transferência" })).not.toBeInTheDocument();
  });

  it("shows the letter beside each item that has one, without making it part of the name", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));

    const item = screen.getByRole("menuitem", { name: "Novo lançamento" });
    expect(item).toHaveAttribute("aria-keyshortcuts", "Alt+L");
    expect(item).toHaveTextContent("Alt+L");
  });

  it("closes the menu when a shortcut is used while it is open", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");
    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));
    expect(screen.getByRole("menu")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "l", code: "KeyL", altKey: true });

    expect(await screen.findByRole("dialog", { name: "Novo lançamento" })).toBeInTheDocument();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("ignores a key held down", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");
    await screen.findByRole("button", { name: "Novo registro" });

    fireEvent.keyDown(window, { key: "l", code: "KeyL", altKey: true, repeat: true });

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
});
