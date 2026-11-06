import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, vi } from "vitest";
import { App } from "./App";
import type { Entry, PaymentTerm, BankAccount, Category, CostCenter, PaymentMethod, DocumentCheck, HistoryEntry, Party, Product, Profile } from "./api";
import { AuthProvider } from "./auth";
import { SOURCE_URL } from "./components/Footer";

const OWNER_PERMISSIONS = {
  parties: "write",
  products: "write",
  finance: "write",
  members: "write",
} as const;

const profile: Profile = {
  user: { id: "u1", name: "Ana", email: "ana@example.com" },
  companies: [{ id: "c1", name: "Padaria", role: "owner", permissions: OWNER_PERMISSIONS }],
};

function party(name: string): Party {
  return {
    id: name,
    company_id: "c1",
    name,
    document: null,
    email: null,
    is_customer: true,
    is_supplier: false,
    is_carrier: false,
    is_seller: false,
    address: null,
    contacts: [],
    tags: [],
    document_check: { status: "not_applicable", legal_name: null, situation: null, needs_attention: false },
  };
}

function partyWithCheck(check: DocumentCheck): Party {
  return { ...party("Padaria Central"), document: "12ABC34501DE35", document_check: check };
}

type Reply = { status: number; body?: unknown };
type Handler = (url: string, init?: RequestInit) => Reply;

function mockApi(handler: Handler) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const { status, body } = handler(String(input), init);
    return new Response(body === undefined ? null : JSON.stringify(body), { status });
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <AuthProvider>
        <App />
      </AuthProvider>
    </MemoryRouter>,
  );
}

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

describe("App", () => {
  it("sends anonymous visitors to the sign-in page", async () => {
    mockApi(() => ({ status: 401, body: { error: "authentication required" } }));
    renderAt("/");
    expect(await screen.findByRole("heading", { name: "Entrar" })).toBeInTheDocument();
  });

  it("shows the password only while the toggle is pressed", async () => {
    mockApi(() => ({ status: 401 }));
    renderAt("/login");

    const password = await screen.findByLabelText("Senha");
    expect(password).toHaveAttribute("type", "password");

    await userEvent.click(screen.getByRole("button", { name: "Mostrar senha" }));
    expect(password).toHaveAttribute("type", "text");
    expect(screen.getByRole("button", { name: "Ocultar senha" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );

    await userEvent.click(screen.getByRole("button", { name: "Ocultar senha" }));
    expect(password).toHaveAttribute("type", "password");
  });

  it("always links to the source code in the footer", async () => {
    mockApi(() => ({ status: 401 }));
    renderAt("/login");
    const link = await screen.findByRole("link", { name: "Código-fonte" });
    expect(link).toHaveAttribute("href", SOURCE_URL);
  });

  it("shows server errors when signing in fails", async () => {
    mockApi((url) =>
      url === "/api/auth/me"
        ? { status: 401 }
        : { status: 401, body: { error: "invalid email or password" } },
    );
    renderAt("/login");
    await userEvent.type(await screen.findByLabelText("E-mail"), "ana@example.com");
    await userEvent.type(screen.getByLabelText("Senha"), "wrong password");
    await userEvent.click(screen.getByRole("button", { name: "Entrar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("invalid email or password");
  });

  it("opens the only company's dashboard and reaches its parties, which load more with the cursor", async () => {
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("after=")) {
        return { status: 200, body: { items: [party("Bruno")], next_cursor: null } };
      }
      return { status: 200, body: { items: [party("Carla")], next_cursor: "id|Carla & Filhos" } };
    });
    renderAt("/");

    expect(await screen.findByRole("heading", { name: "Painel" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("link", { name: "Clientes e fornecedores" }));
    expect(await screen.findByRole("heading", { name: "Clientes e fornecedores" })).toBeInTheDocument();
    expect(await screen.findByText("Carla")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Carregar mais" }));
    expect(await screen.findByText("Bruno")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Carregar mais" })).not.toBeInTheDocument();

    // The cursor contains `&` and `|`, so it must be percent-encoded.
    const requested = fetchMock.mock.calls.map(([url]) => String(url));
    expect(requested).toContain("/api/companies/c1/parties?after=id%7CCarla+%26+Filhos");
  });

  describe("filtering and sorting the list", () => {
    const listUrls = (fetchMock: ReturnType<typeof mockApi>) =>
      fetchMock.mock.calls.map(([url]) => String(url)).filter((url) => url.includes("/parties"));

    it("sends the search, kind and order, and keeps them when loading more", async () => {
      const fetchMock = mockApi((url) => {
        if (url === "/api/auth/me") return { status: 200, body: profile };
        if (url.includes("after=")) {
          return { status: 200, body: { items: [party("Ana Maria")], next_cursor: null } };
        }
        if (url.includes("q=ana")) {
          return { status: 200, body: { items: [party("Ana"), party("Anabela")], next_cursor: "id|Anabela" } };
        }
        return { status: 200, body: { items: [party("Carla")], next_cursor: null } };
      });
      renderAt("/companies/c1/parties");
      expect(await screen.findByText("Carla")).toBeInTheDocument();

      await userEvent.type(screen.getByRole("searchbox", { name: "Buscar" }), "  ana ");
      await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "supplier");
      await userEvent.selectOptions(screen.getByLabelText("Ordem"), "-name");
      expect(await screen.findByText("Anabela")).toBeInTheDocument();
      expect(screen.queryByText("Carla")).not.toBeInTheDocument();

      await userEvent.click(screen.getByRole("button", { name: "Carregar mais" }));
      expect(await screen.findByText("Ana Maria")).toBeInTheDocument();

      // One request per settled filter, not one per keystroke, and the cursor travels with them.
      const urls = listUrls(fetchMock);
      expect(urls).toContain("/api/companies/c1/parties?q=ana&kind=supplier&sort=-name");
      expect(urls).toContain("/api/companies/c1/parties?q=ana&kind=supplier&sort=-name&after=id%7CAnabela");
      expect(urls.some((url) => url.includes("q=a&") || url.endsWith("q=a"))).toBe(false);
    });

    it("ignores a slow answer to an earlier filter", async () => {
      let answerFirstSearch: (response: Response) => void = () => {};
      vi.stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL) => {
          const url = String(input);
          const reply = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });
          if (url === "/api/auth/me") return reply(profile);
          if (url.includes("kind=customer")) {
            return reply({ items: [party("Zeca Comprador")], next_cursor: null });
          }
          if (url.includes("kind=supplier")) {
            return new Promise<Response>((resolve) => (answerFirstSearch = resolve));
          }
          return reply({ items: [party("Todos")], next_cursor: null });
        }),
      );
      renderAt("/companies/c1/parties");
      expect(await screen.findByText("Todos")).toBeInTheDocument();

      await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "supplier");
      await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "customer");
      expect(await screen.findByText("Zeca Comprador")).toBeInTheDocument();

      answerFirstSearch(
        new Response(JSON.stringify({ items: [party("Vera Vendedora")], next_cursor: null }), { status: 200 }),
      );
      await new Promise((resolve) => setTimeout(resolve, 20));
      expect(screen.queryByText("Vera Vendedora")).not.toBeInTheDocument();
      expect(screen.getByText("Zeca Comprador")).toBeInTheDocument();
    });

    it("offers no stale cursor while the new filter loads, and no old rows if it fails", async () => {
      let failNext = false;
      vi.stubGlobal(
        "fetch",
        vi.fn(async (input: RequestInfo | URL) => {
          const url = String(input);
          if (url === "/api/auth/me") return new Response(JSON.stringify(profile), { status: 200 });
          if (url.includes("kind=supplier")) {
            return failNext
              ? new Response(JSON.stringify({ error: "boom" }), { status: 500 })
              : new Promise<Response>(() => {});
          }
          return new Response(
            JSON.stringify({ items: [party("Carla")], next_cursor: "id|Carla" }),
            { status: 200 },
          );
        }),
      );
      renderAt("/companies/c1/parties");
      expect(await screen.findByText("Carla")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Carregar mais" })).toBeInTheDocument();

      // The request for the new filter never answers: the old cursor must not be offered.
      await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "supplier");
      await waitFor(() =>
        expect(screen.queryByRole("button", { name: "Carregar mais" })).not.toBeInTheDocument(),
      );

      // When it fails, the old filter's rows go away instead of standing in for the new one.
      failNext = true;
      await userEvent.selectOptions(screen.getByLabelText("Ordem"), "-name");
      expect(await screen.findByRole("alert")).toHaveTextContent("boom");
      expect(screen.queryByText("Carla")).not.toBeInTheDocument();
    });

    it("says nothing matched and clears the filters from there", async () => {
      mockApi((url) => {
        if (url === "/api/auth/me") return { status: 200, body: profile };
        if (url.includes("q=zzz")) return { status: 200, body: { items: [], next_cursor: null } };
        return { status: 200, body: { items: [party("Carla")], next_cursor: null } };
      });
      renderAt("/companies/c1/parties");
      expect(await screen.findByText("Carla")).toBeInTheDocument();

      await userEvent.type(screen.getByRole("searchbox", { name: "Buscar" }), "zzz");
      expect(await screen.findByText("Nenhum cadastro encontrado.")).toBeInTheDocument();

      await userEvent.click(screen.getByRole("button", { name: "Limpar filtros" }));
      expect(await screen.findByText("Carla")).toBeInTheDocument();
      expect(screen.getByRole("searchbox", { name: "Buscar" })).toHaveValue("");
    });
  });

  it("disables the submit button while signing in and clears the old error on retry", async () => {
    let attempts = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn((input: RequestInfo | URL) => {
        if (String(input) === "/api/auth/me") {
          return Promise.resolve(new Response(null, { status: 401 }));
        }
        attempts += 1;
        // The first attempt fails; the second never answers, so we can inspect the pending state.
        return attempts === 1
          ? Promise.resolve(
              new Response(JSON.stringify({ error: "invalid email or password" }), { status: 401 }),
            )
          : new Promise<Response>(() => {});
      }),
    );
    renderAt("/login");

    await userEvent.type(await screen.findByLabelText("E-mail"), "ana@example.com");
    await userEvent.type(screen.getByLabelText("Senha"), "wrong password");
    await userEvent.click(screen.getByRole("button", { name: "Entrar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("invalid email or password");

    await userEvent.click(screen.getByRole("button", { name: "Entrar" }));
    const submit = screen.getByRole("button", { name: "Entrar" });
    expect(submit).toBeDisabled();
    expect(submit).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("offers a retry when the first page of parties cannot be loaded", async () => {
    let listCalls = 0;
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      listCalls += 1;
      return listCalls === 1
        ? { status: 500, body: { error: "internal error" } }
        : { status: 200, body: { items: [party("Carla")], next_cursor: null } };
    });
    renderAt("/companies/c1/parties");

    expect(await screen.findByRole("alert")).toHaveTextContent("internal error");
    expect(screen.queryByRole("table")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByText("Carla")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("signs up and lands on the new company's dashboard", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 401 };
      if (url === "/api/auth/register") return { status: 201, body: profile };
      expect(init?.method ?? "GET").toBe("GET");
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/register");

    await userEvent.type(await screen.findByLabelText("Seu nome"), "Ana");
    await userEvent.type(screen.getByLabelText("Nome da empresa"), "Padaria");
    await userEvent.type(screen.getByLabelText("E-mail"), "ana@example.com");
    await userEvent.type(screen.getByLabelText("Senha"), "correct horse");
    await userEvent.click(screen.getByRole("button", { name: "Criar conta" }));

    expect(await screen.findByRole("heading", { name: "Painel" })).toBeInTheDocument();
    const register = fetchMock.mock.calls.find(([url]) => String(url) === "/api/auth/register");
    expect(JSON.parse(String(register?.[1]?.body))).toEqual({
      name: "Ana",
      email: "ana@example.com",
      password: "correct horse",
      company_name: "Padaria",
    });
  });

  it("lets users with several companies choose one", async () => {
    mockApi((url) =>
      url === "/api/auth/me"
        ? {
            status: 200,
            body: {
              ...profile,
              companies: [
                { id: "c1", name: "Padaria", role: "owner", permissions: OWNER_PERMISSIONS },
                { id: "c2", name: "Mercearia", role: "viewer", permissions: { parties: "read" } },
              ],
            },
          }
        : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/");

    expect(await screen.findByRole("heading", { name: "Escolha uma empresa" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("link", { name: "Mercearia" }));
    expect(await screen.findByRole("heading", { name: "Painel" })).toBeInTheDocument();
    expect(screen.getByText("Mercearia")).toBeInTheDocument();
  });

  it("returns to sign-in when the session is lost after the page loaded", async () => {
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") {
        return { status: 401, body: { error: "authentication required" } };
      }
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/parties");

    await userEvent.type(await screen.findByLabelText("Nome"), "Zeca");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));
    expect(await screen.findByRole("heading", { name: "Entrar" })).toBeInTheDocument();
  });

  it("keeps the user signed in and says so when sign-out fails", async () => {
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 500, body: { error: "internal error" } };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/parties");

    await userEvent.click(await screen.findByRole("button", { name: "Sair" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Não foi possível sair");
    expect(screen.getByRole("button", { name: "Sair" })).toBeInTheDocument();
  });

  it("returns to sign-in after a successful sign-out", async () => {
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 204 };
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/parties");

    await userEvent.click(await screen.findByRole("button", { name: "Sair" }));
    expect(await screen.findByRole("heading", { name: "Entrar" })).toBeInTheDocument();
  });

  describe("registering a CNPJ", () => {
    // Registers a party whose document comes back with the given registry result, and returns
    // the body that was sent to the API.
    async function registerWith(check: DocumentCheck) {
      const created = partyWithCheck(check);
      let listed: Party[] = [];
      const fetchMock = mockApi((url, init) => {
        if (url === "/api/auth/me") return { status: 200, body: profile };
        if (init?.method === "POST") {
          listed = [created];
          return { status: 201, body: created };
        }
        return { status: 200, body: { items: listed, next_cursor: null } };
      });
      renderAt("/companies/c1/parties");

      await userEvent.type(await screen.findByLabelText("Nome"), "Padaria Central");
      // Letters must be accepted: the CNPJ is alphanumeric since July 2026.
      await userEvent.type(screen.getByLabelText("CPF ou CNPJ"), "12.ABC.345/01DE-35");
      await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));

      const post = fetchMock.mock.calls.find(([, init]) => init?.method === "POST");
      return JSON.parse(String(post?.[1]?.body));
    }

    it("sends the letters of an alphanumeric CNPJ and confirms an active company", async () => {
      const sent = await registerWith({
        status: "verified",
        legal_name: "PADARIA CENTRAL LTDA",
        situation: "ATIVA",
        needs_attention: false,
      });
      expect(sent.document).toBe("12.ABC.345/01DE-35");
      expect(await screen.findByText("CNPJ verificado: PADARIA CENTRAL LTDA.")).toBeInTheDocument();
      const badge = await screen.findByText("Verificado");
      // The legal name is part of the badge's text for screen readers, not only a hover title.
      expect(badge).toHaveTextContent("Verificado (PADARIA CENTRAL LTDA)");
    });

    it("warns, without blocking, when the company is not active", async () => {
      await registerWith({
        status: "verified",
        legal_name: "PADARIA CENTRAL LTDA",
        situation: "BAIXADA",
        needs_attention: true,
      });
      expect(await screen.findByRole("status")).toHaveTextContent(
        "Atenção: PADARIA CENTRAL LTDA está com situação BAIXADA na Receita.",
      );
      // The party was saved and is listed, with its situation as the badge.
      expect(await screen.findByText("BAIXADA")).toBeInTheDocument();
    });

    it("keeps a CNPJ the registry does not know, and asks the user to check the number", async () => {
      await registerWith({
        status: "not_found",
        legal_name: null,
        situation: null,
        needs_attention: true,
      });
      expect(await screen.findByRole("status")).toHaveTextContent(
        "a Receita não encontrou este CNPJ",
      );
      // Saved and listed, not refused.
      expect(await screen.findByText("Não encontrado")).toBeInTheDocument();
    });

    it("says so when the registry could not be reached", async () => {
      await registerWith({
        status: "unverified",
        legal_name: null,
        situation: null,
        needs_attention: true,
      });
      expect(await screen.findByRole("status")).toHaveTextContent(
        "não foi possível consultar o CNPJ na Receita",
      );
      expect(await screen.findByText("Não verificado")).toBeInTheDocument();
    });
  });

  describe("the history of a party", () => {
    const created: HistoryEntry = {
      at: Date.UTC(2026, 10, 5, 12, 0),
      actor: { kind: "user", id: "u1", name: "Ana" },
      action: "created",
      changes: [
        { field: "name", from: null, to: "Padaria Central" },
        { field: "is_customer", from: null, to: "true" },
        { field: "is_supplier", from: null, to: "false" },
        { field: "document_check", from: null, to: "unverified" },
      ],
    };
    const rechecked: HistoryEntry = {
      at: Date.UTC(2026, 10, 5, 13, 30),
      actor: { kind: "user", id: "u2", name: "Bia" },
      action: "updated",
      changes: [{ field: "document_check", from: "unverified", to: "verified" }],
    };

    function listWithHistory(history: () => Reply) {
      const listed = partyWithCheck({
        status: "unverified",
        legal_name: null,
        situation: null,
        needs_attention: true,
      });
      const fetchMock = mockApi((url) => {
        if (url === "/api/auth/me") return { status: 200, body: profile };
        if (url.endsWith("/history")) return history();
        return { status: 200, body: { items: [listed], next_cursor: null } };
      });
      renderAt("/companies/c1/parties");
      return { listed, fetchMock };
    }

    it("lists who did what in words, newest first", async () => {
      const { listed, fetchMock } = listWithHistory(() => ({
        status: 200,
        body: [created, rechecked],
      }));

      await userEvent.click(await screen.findByRole("button", { name: "Histórico de Padaria Central" }));

      const items = await screen.findAllByRole("listitem");
      const text = items.map((item) => item.textContent ?? "");
      // Newest first: Bia's re-check, then Ana's registration.
      const bia = text.findIndex((line) => line.includes("Bia alterou"));
      const ana = text.findIndex((line) => line.includes("Ana cadastrou"));
      expect(bia).toBeGreaterThanOrEqual(0);
      expect(bia).toBeLessThan(ana);
      // Portuguese labels and statuses, never the stored keys.
      expect(screen.getByText("Não verificado → Verificado")).toBeInTheDocument();
      expect(screen.getByText("Fornecedor:")).toBeInTheDocument();
      expect(screen.queryByText(/is_supplier|document_check/)).not.toBeInTheDocument();
      expect(String(fetchMock.mock.calls.find(([url]) => String(url).endsWith("/history"))?.[0])).toBe(
        `/api/companies/c1/parties/${listed.id}/history`,
      );
    });

    it("loads only when opened, and closes again", async () => {
      const { fetchMock } = listWithHistory(() => ({ status: 200, body: [created] }));
      const opener = await screen.findByRole("button", { name: "Histórico de Padaria Central" });
      expect(fetchMock.mock.calls.some(([url]) => String(url).endsWith("/history"))).toBe(false);

      await userEvent.click(opener);
      expect(opener).toHaveAttribute("aria-expanded", "true");
      expect(await screen.findByText(/cadastrou/)).toBeInTheDocument();

      await userEvent.click(opener);
      expect(opener).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByText(/cadastrou/)).not.toBeInTheDocument();
    });

    it("shows the API's reason when the history cannot be loaded, and can try again", async () => {
      let calls = 0;
      listWithHistory(() => {
        calls += 1;
        return calls === 1
          ? { status: 500, body: { error: "internal error" } }
          : { status: 200, body: [created] };
      });
      const opener = await screen.findByRole("button", { name: "Histórico de Padaria Central" });
      await userEvent.click(opener);
      expect(await screen.findByRole("alert")).toHaveTextContent("internal error");

      await userEvent.click(screen.getByRole("button", { name: "Tentar de novo" }));
      expect(await screen.findByText(/cadastrou/)).toBeInTheDocument();
      expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });

    it("explains an empty history instead of showing nothing", async () => {
      listWithHistory(() => ({ status: 200, body: [] }));
      await userEvent.click(await screen.findByRole("button", { name: "Histórico de Padaria Central" }));
      expect(await screen.findByText(/anterior ao histórico/)).toBeInTheDocument();
    });

    it("reloads an open history after the party is checked again", async () => {
      const unverifiedParty = partyWithCheck({
        status: "unverified",
        legal_name: null,
        situation: null,
        needs_attention: true,
      });
      const verifiedParty = partyWithCheck({
        status: "verified",
        legal_name: "PADARIA CENTRAL LTDA",
        situation: "ATIVA",
        needs_attention: false,
      });
      let historyCalls = 0;
      mockApi((url, init) => {
        if (url === "/api/auth/me") return { status: 200, body: profile };
        if (url.endsWith("/history")) {
          historyCalls += 1;
          return { status: 200, body: historyCalls === 1 ? [created] : [created, rechecked] };
        }
        if (init?.method === "POST") return { status: 200, body: verifiedParty };
        return { status: 200, body: { items: [unverifiedParty], next_cursor: null } };
      });
      renderAt("/companies/c1/parties");

      await userEvent.click(await screen.findByRole("button", { name: "Histórico de Padaria Central" }));
      expect(await screen.findByText(/cadastrou/)).toBeInTheDocument();
      expect(screen.queryByText(/alterou/)).not.toBeInTheDocument();

      await userEvent.click(screen.getByRole("button", { name: /Verificar de novo/ }));
      // The open panel follows the re-check and now lists the change too.
      expect(await screen.findByText(/alterou/)).toBeInTheDocument();
      expect(historyCalls).toBe(2);
    });

    it("names the panel and ties the toggle to it", async () => {
      listWithHistory(() => ({ status: 200, body: [created] }));
      const opener = await screen.findByRole("button", { name: "Histórico de Padaria Central" });
      await userEvent.click(opener);

      const panel = await screen.findByRole("region", { name: "Histórico de Padaria Central" });
      expect(opener.getAttribute("aria-controls")).toBe(panel.closest("tr")?.id);
    });
  });

  describe("checking a CNPJ again", () => {
    const unverified: DocumentCheck = {
      status: "unverified",
      legal_name: null,
      situation: null,
      needs_attention: true,
    };

    // Lists one unverified party; `answer` is what the re-check endpoint replies.
    function listWithAnUnverifiedParty(answer: (party: Party) => Reply) {
      const listed = partyWithCheck(unverified);
      const fetchMock = mockApi((url, init) => {
        if (url === "/api/auth/me") return { status: 200, body: profile };
        if (init?.method === "POST") return answer(listed);
        return { status: 200, body: { items: [listed], next_cursor: null } };
      });
      renderAt("/companies/c1/parties");
      return { listed, fetchMock };
    }

    it("replaces the old answer and hides the button once the company is verified", async () => {
      const { listed, fetchMock } = listWithAnUnverifiedParty((party) => ({
        status: 200,
        body: {
          ...party,
          document_check: {
            status: "verified",
            legal_name: "PADARIA CENTRAL LTDA",
            situation: "ATIVA",
            needs_attention: false,
          },
        },
      }));

      await userEvent.click(await screen.findByRole("button", { name: /Verificar de novo/ }));

      expect(await screen.findByText("Padaria Central: CNPJ verificado: PADARIA CENTRAL LTDA.")).toBeInTheDocument();
      expect(await screen.findByText("Verificado")).toBeInTheDocument();
      expect(screen.queryByText("Não verificado")).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /Verificar de novo/ })).not.toBeInTheDocument();
      const post = fetchMock.mock.calls.find(([, init]) => init?.method === "POST");
      expect(String(post?.[0])).toBe(`/api/companies/c1/parties/${listed.id}/document-check`);
    });

    it("names the party in each button, so rows can be told apart", async () => {
      listWithAnUnverifiedParty((party) => ({ status: 200, body: party }));
      expect(
        await screen.findByRole("button", {
          name: "Verificar de novo o CNPJ de Padaria Central",
        }),
      ).toBeInTheDocument();
    });

    it("keeps the button and says so when the registry is still unreachable", async () => {
      listWithAnUnverifiedParty((party) => ({ status: 200, body: party }));

      await userEvent.click(await screen.findByRole("button", { name: /Verificar de novo/ }));

      expect(await screen.findByRole("status")).toHaveTextContent("a Receita continua indisponível");
      expect(screen.getByRole("button", { name: /Verificar de novo/ })).toBeInTheDocument();
    });

    it("shows the API's reason when the check is refused", async () => {
      listWithAnUnverifiedParty(() => ({
        status: 409,
        body: { error: "CNPJ lookup is switched off on this installation" },
      }));

      await userEvent.click(await screen.findByRole("button", { name: /Verificar de novo/ }));

      expect(await screen.findByRole("alert")).toHaveTextContent("CNPJ lookup is switched off");
    });
  });

  it("shows the API message when a party is rejected", async () => {
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") {
        return { status: 422, body: { error: "document is not a valid CPF or CNPJ" } };
      }
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/parties");

    await userEvent.type(await screen.findByLabelText("Nome"), "Padaria Central");
    await userEvent.type(screen.getByLabelText("CPF ou CNPJ"), "123");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("document is not a valid CPF or CNPJ"),
    );
  });
});

describe("products and services", () => {
  function product(name: string, overrides: Partial<Product> = {}): Product {
    return {
      id: `p-${name}`,
      company_id: "c1",
      kind: "product",
      name,
      sku: null,
      unit: "UN",
      ncm: null,
      sale_price: null,
      cost_price: null,
      minimum_stock: null,
      active: true,
      ...overrides,
    };
  }

  const page = (...items: Product[]) => ({ status: 200, body: { items, next_cursor: null } });
  const bodyOf = (fetchMock: ReturnType<typeof mockApi>, method: string) => {
    const call = fetchMock.mock.calls.find(([, init]) => init?.method === method);
    return JSON.parse(String(call?.[1]?.body));
  };

  it("lists them with prices written the Brazilian way and reaches them from the parties page", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/products")) {
        return page(
          product("Pão francês", { sku: "PAO-1", sale_price: "1234.5", cost_price: "7", minimum_stock: "10" }),
          product("Entrega", { kind: "service", active: false }),
        );
      }
      return { status: 200, body: { items: [], next_cursor: null } };
    });
    renderAt("/companies/c1/parties");
    await userEvent.click(await screen.findByRole("link", { name: "Produtos e serviços" }));

    const row = (await screen.findByText("Pão francês")).closest("tr")!;
    expect(row).toHaveTextContent("PAO-1");
    expect(row).toHaveTextContent("R$ 1.234,50");
    expect(row).toHaveTextContent("R$ 7,00");
    expect(row).toHaveTextContent("Ativo");
    const service = screen.getByText("Entrega").closest("tr")!;
    expect(service).toHaveTextContent("Serviço");
    expect(service).toHaveTextContent("Inativo");
  });

  it("sends prices as plain decimals, whatever way they were typed", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 201, body: product("Bolo") };
      return page();
    });
    renderAt("/companies/c1/products");

    await userEvent.type(await screen.findByLabelText("Nome"), "Bolo");
    await userEvent.type(screen.getByLabelText("SKU"), "bolo-1");
    await userEvent.type(screen.getByLabelText("NCM"), "1905.90.90");
    await userEvent.type(screen.getByLabelText("Preço de venda (R$)"), "1.234,50");
    await userEvent.type(screen.getByLabelText("Preço de custo (R$)"), "7");
    await userEvent.type(screen.getByLabelText("Estoque mínimo"), "2,5");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByText("Bolo cadastrado.")).toBeInTheDocument();
    expect(bodyOf(fetchMock, "POST")).toEqual({
      kind: "product",
      name: "Bolo",
      sku: "bolo-1",
      unit: "UN",
      ncm: "1905.90.90",
      sale_price: "1234.50",
      cost_price: "7",
      minimum_stock: "2.5",
      active: true,
    });
  });

  it("leaves out the NCM and the minimum stock of a service", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 201, body: product("Entrega", { kind: "service" }) };
      return page();
    });
    renderAt("/companies/c1/products");

    await userEvent.selectOptions(await screen.findByLabelText("Tipo"), "service");
    expect(screen.getByLabelText("NCM")).toBeDisabled();
    expect(screen.getByLabelText("Estoque mínimo")).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Nome"), "Entrega");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByText("Entrega cadastrado.")).toBeInTheDocument();
    const sent = bodyOf(fetchMock, "POST");
    expect(sent.kind).toBe("service");
    expect(sent).not.toHaveProperty("ncm");
    expect(sent).not.toHaveProperty("minimum_stock");
  });

  it("sends the whole product on a change, a cleared price as absent and the unchecked flag as false", async () => {
    const bread = product("Pão", { sale_price: "12.5", cost_price: "7", active: true });
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") return { status: 200, body: bread };
      return page(bread);
    });
    renderAt("/companies/c1/products");

    await userEvent.click(await screen.findByRole("button", { name: "Alterar Pão" }));
    await userEvent.clear(screen.getByLabelText("Preço de custo (R$)"));
    await userEvent.click(screen.getByRole("checkbox", { name: "Ativo" }));
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));

    await screen.findByText("Pão atualizado.");
    expect(bodyOf(fetchMock, "PUT")).toEqual({
      kind: "product",
      name: "Pão",
      unit: "UN",
      sale_price: "12.5",
      active: false,
    });
  });

  it("keeps the list where it was after a change, and forgets an error when the change is cancelled", async () => {
    const first = product("Clara");
    const second = product("Davi", { sale_price: "1" });
    let putStatus = 409;
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") {
        return putStatus === 200
          ? { status: 200, body: { ...second, sale_price: "2" } }
          : { status: 409, body: { error: "a product with this SKU already exists" } };
      }
      if (url.includes("after=")) return page(second);
      return { status: 200, body: { items: [first], next_cursor: "id|Clara" } };
    });
    renderAt("/companies/c1/products");

    await userEvent.click(await screen.findByRole("button", { name: "Carregar mais" }));
    await userEvent.click(await screen.findByRole("button", { name: "Alterar Davi" }));
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("SKU already exists");

    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();

    putStatus = 200;
    await userEvent.click(screen.getByRole("button", { name: "Alterar Davi" }));
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));
    expect(await screen.findByText("Davi atualizado.")).toBeInTheDocument();
    // Both pages are still there, and no request went back to the first page.
    expect(screen.getByText("Clara")).toBeInTheDocument();
    expect(screen.getByText("R$ 2,00")).toBeInTheDocument();
    const firstPages = fetchMock.mock.calls.filter(
      ([url, init]) => String(url).endsWith("/products") && init?.method === undefined,
    );
    expect(firstPages).toHaveLength(1);
  });

  it("does not rewrite a price written the English way", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") {
        return { status: 422, body: { error: "sale price must be a decimal" } };
      }
      return page();
    });
    renderAt("/companies/c1/products");

    await userEvent.type(await screen.findByLabelText("Nome"), "Bolo");
    await userEvent.type(screen.getByLabelText("Preço de venda (R$)"), "1,234.56");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("sale price must be");
    expect(bodyOf(fetchMock, "POST").sale_price).toBe("1,234.56");
  });

  it("asks what was meant by 1.234 instead of guessing, and sends nothing", async () => {
    const fetchMock = mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : page(),
    );
    renderAt("/companies/c1/products");

    await userEvent.type(await screen.findByLabelText("Nome"), "Bolo");
    await userEvent.type(screen.getByLabelText("Preço de venda (R$)"), "1.234");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("pode ser lido de dois jeitos");
    expect(fetchMock.mock.calls.some(([, init]) => init?.method === "POST")).toBe(false);

    await userEvent.clear(screen.getByLabelText("Preço de venda (R$)"));
    await userEvent.type(screen.getByLabelText("Preço de venda (R$)"), "1.234,00");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));
    await waitFor(() =>
      expect(fetchMock.mock.calls.some(([, init]) => init?.method === "POST")).toBe(true),
    );
  });

  it("shows what the API refused, next to the form", async () => {
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") {
        return { status: 422, body: { error: "sale price must be a decimal with up to 13 integer and 6 fractional digits, written like 10.50" } };
      }
      return page();
    });
    renderAt("/companies/c1/products");

    await userEvent.type(await screen.findByLabelText("Nome"), "Bolo");
    await userEvent.type(screen.getByLabelText("Preço de venda (R$)"), "abc");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("sale price must be a decimal");
  });

  it("changes a product with the form filled in, and the kind stays as it was", async () => {
    const bread = product("Pão", { sale_price: "12.5", minimum_stock: "3", sku: "PAO-1" });
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") return { status: 200, body: { ...bread, name: "Pão francês" } };
      return page(bread);
    });
    renderAt("/companies/c1/products");

    await userEvent.click(await screen.findByRole("button", { name: "Alterar Pão" }));
    expect(screen.getByRole("heading", { name: "Alterar Pão" })).toBeInTheDocument();
    expect(screen.getByLabelText("Tipo")).toBeDisabled();
    expect(screen.getByLabelText("Preço de venda (R$)")).toHaveValue("12,5");
    expect(screen.getByLabelText("SKU")).toHaveValue("PAO-1");

    await userEvent.clear(screen.getByLabelText("Nome"));
    await userEvent.type(screen.getByLabelText("Nome"), "Pão francês");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));

    expect(await screen.findByText("Pão francês atualizado.")).toBeInTheDocument();
    const put = fetchMock.mock.calls.find(([, init]) => init?.method === "PUT")!;
    expect(String(put[0])).toBe("/api/companies/c1/products/p-Pão");
    expect(JSON.parse(String(put[1]?.body))).toMatchObject({
      name: "Pão francês",
      sale_price: "12.5",
      minimum_stock: "3",
      sku: "PAO-1",
    });
    // Back to registering.
    expect(screen.getByRole("heading", { name: "Novo produto ou serviço" })).toBeInTheDocument();
  });

  it("asks for the active ones of a kind, matching a text, in the order chosen", async () => {
    const fetchMock = mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : page(product("Pão")),
    );
    renderAt("/companies/c1/products");
    await screen.findByText("Pão");

    await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "service");
    await userEvent.selectOptions(screen.getByLabelText("Situação"), "true");
    await userEvent.selectOptions(screen.getByLabelText("Ordem"), "-name");
    await userEvent.type(screen.getByRole("searchbox", { name: "Buscar" }), "ent");

    await waitFor(() => {
      const urls = fetchMock.mock.calls.map(([url]) => String(url));
      expect(urls).toContain("/api/companies/c1/products?q=ent&kind=service&active=true&sort=-name");
    });
  });

  it("shows the history of a product with its prices written as money", async () => {
    const bread = product("Pão");
    const entry: HistoryEntry = {
      at: Date.UTC(2026, 10, 5, 12, 0),
      actor: { kind: "user", id: "u1", name: "Ana" },
      action: "updated",
      changes: [
        { field: "sale_price", from: "12.5", to: "15" },
        { field: "active", from: "true", to: "false" },
      ],
    };
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/history")) return { status: 200, body: [entry] };
      return page(bread);
    });
    renderAt("/companies/c1/products");

    await userEvent.click(await screen.findByRole("button", { name: "Histórico de Pão" }));
    const region = await screen.findByRole("region", { name: "Histórico de Pão" });
    expect(await within(region).findByText(/R\$ 12,50 → R\$ 15,00/)).toBeInTheDocument();
    expect(within(region).getByText(/sim → não/)).toBeInTheDocument();
  });
});

describe("dashboard", () => {
  beforeEach(() => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : { status: 404 }));
  });

  it("says its numbers are sample data", async () => {
    renderAt("/companies/c1");
    expect(await screen.findByText(/Dados de exemplo/)).toBeInTheDocument();
  });

  it("writes the cash balance as money", async () => {
    renderAt("/companies/c1");
    expect(await screen.findByText("R$ 48.720,35")).toBeInTheDocument();
  });

  it("scopes the chart table to the period chosen", async () => {
    renderAt("/companies/c1");
    await screen.findByRole("heading", { name: "Painel" });

    await userEvent.click(screen.getAllByRole("button", { name: "Ver como tabela" })[0]);
    const flow = screen.getByRole("table", { name: "Entradas e saídas" });
    expect(within(flow).getAllByRole("row")).toHaveLength(1 + 6);

    await userEvent.click(screen.getByLabelText("12 meses"));
    expect(within(flow).getAllByRole("row")).toHaveLength(1 + 12);
  });

  it("offers the due ranges as a table, with overdue named in words", async () => {
    renderAt("/companies/c1");
    await screen.findByRole("heading", { name: "Painel" });

    await userEvent.click(screen.getAllByRole("button", { name: "Ver como tabela" })[1]);
    const due = screen.getByRole("table", { name: "Contas por vencimento" });
    expect(within(due).getByRole("cell", { name: "Vencidas" })).toBeInTheDocument();
    expect(within(due).getByRole("cell", { name: "R$ 21.350,50" })).toBeInTheDocument();
  });

  it("shows every series of a month, in money, when the chart is focused and moved with the arrows", async () => {
    renderAt("/companies/c1");
    const chart = await screen.findByRole("group", { name: /Entradas e saídas por mês/ });

    chart.focus();
    expect(await screen.findByText("R$ 54.300,00")).toBeInTheDocument();
    expect(screen.getByText("R$ 46.780,00")).toBeInTheDocument();

    await userEvent.keyboard("{ArrowLeft}");
    expect(await screen.findByText("R$ 50.480,00")).toBeInTheDocument();
  });
});

describe("cost centers", () => {
  function costCenter(name: string, active = true): CostCenter {
    return { id: `cc-${name}`, company_id: "c1", name, active };
  }

  const page = (...items: CostCenter[]) => ({ status: 200, body: { items, next_cursor: null } });
  const bodyOf = (fetchMock: ReturnType<typeof mockApi>, method: string) => {
    const call = fetchMock.mock.calls.find(([, init]) => init?.method === method);
    return JSON.parse(String(call?.[1]?.body));
  };

  it("registers one, changes it in place and filters the list", async () => {
    const store = costCenter("Loja Centro");
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/parties")) return { status: 200, body: { items: [], next_cursor: null } };
      if (init?.method === "POST") return { status: 201, body: costCenter("Projeto X") };
      if (init?.method === "PUT") return { status: 200, body: { ...store, name: "Loja Norte" } };
      return page(store);
    });
    renderAt("/companies/c1/parties");
    await userEvent.click(await screen.findByRole("link", { name: "Centros de custo" }));
    expect(await screen.findByText("Loja Centro")).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText("Nome"), "Projeto X");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));
    expect(await screen.findByText("Projeto X cadastrado.")).toBeInTheDocument();
    expect(bodyOf(fetchMock, "POST")).toEqual({ name: "Projeto X", active: true });

    await userEvent.click(screen.getByRole("button", { name: "Alterar Loja Centro" }));
    await userEvent.clear(screen.getByLabelText("Nome"));
    await userEvent.type(screen.getByLabelText("Nome"), "Loja Norte");
    await userEvent.click(screen.getByRole("checkbox", { name: "Ativo" }));
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));
    expect(await screen.findByText("Loja Norte atualizado.")).toBeInTheDocument();
    expect(bodyOf(fetchMock, "PUT")).toEqual({ name: "Loja Norte", active: false });

    await userEvent.selectOptions(screen.getByLabelText("Situação"), "false");
    await userEvent.type(screen.getByRole("searchbox", { name: "Buscar" }), "lo");
    await waitFor(() => {
      const urls = fetchMock.mock.calls.map(([url]) => String(url));
      expect(urls).toContain("/api/companies/c1/cost-centers?q=lo&active=false");
    });
  });

  it("shows what the API refused, and the history of one", async () => {
    const store = costCenter("Loja");
    const entry: HistoryEntry = {
      at: Date.UTC(2026, 10, 5, 12, 0),
      actor: { kind: "user", id: "u1", name: "Ana" },
      action: "updated",
      changes: [{ field: "active", from: "true", to: "false" }],
    };
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/history")) return { status: 200, body: [entry] };
      if (init?.method === "POST") return { status: 409, body: { error: "this name is already in use" } };
      return page(store);
    });
    renderAt("/companies/c1/cost-centers");

    await userEvent.type(await screen.findByLabelText("Nome"), "loja");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("already in use");

    await userEvent.click(screen.getByRole("button", { name: "Histórico de Loja" }));
    const region = await screen.findByRole("region", { name: "Histórico de Loja" });
    expect(await within(region).findByText(/sim → não/)).toBeInTheDocument();
  });
});

describe("categories", () => {
  function category(name: string, overrides: Partial<Category> = {}): Category {
    return {
      id: `cat-${name}`,
      company_id: "c1",
      parent_id: null,
      name,
      dre_group: "operating_expenses",
      cash_flow_group: "operating",
      level: 1,
      active: true,
      ...overrides,
    };
  }

  const tree = [
    category("Despesas"),
    category("Aluguel", { parent_id: "cat-Despesas", level: 2 }),
    category("Transferências", { dre_group: "transfers", cash_flow_group: "transfer" }),
  ];
  const listed = (items: Category[]) => ({ status: 200, body: { items } });
  const bodyOf = (fetchMock: ReturnType<typeof mockApi>, method: string) => {
    const call = fetchMock.mock.calls.find(([, init]) => init?.method === method);
    return JSON.parse(String(call?.[1]?.body));
  };

  it("draws the tree with each subcategory under its parent, and the groups in words", async () => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : listed(tree)));
    renderAt("/companies/c1/categories");

    await screen.findByRole("table");
    const rows = screen.getAllByRole("row");
    // The subcategory comes right after its parent, indented one level.
    expect(rows[1]).toHaveTextContent(/^Despesas/);
    expect(rows[2]).toHaveTextContent("Subcategoria de Despesas: Aluguel");
    expect(rows[2]).toHaveTextContent("Despesas operacionais");
    expect(rows[2]).toHaveTextContent("Operacional");
    expect(rows[3]).toHaveTextContent(/^Transferências/);
    expect(within(rows[2]).getAllByRole("cell")[0].style.paddingLeft).toBe("1.75rem");
    expect(within(rows[1]).getAllByRole("cell")[0].style.paddingLeft).toBe("0.5rem");
  });

  it("offers the default tree to a company with none, and shows it once applied", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 201, body: { items: tree } };
      return listed([]);
    });
    renderAt("/companies/c1/categories");

    await userEvent.click(await screen.findByRole("button", { name: "Usar categorias padrão" }));
    expect(await screen.findByText("Transferências")).toBeInTheDocument();
    expect(
      fetchMock.mock.calls.some(([url]) => String(url) === "/api/companies/c1/categories/default-template"),
    ).toBe(true);
    expect(screen.queryByRole("button", { name: "Usar categorias padrão" })).not.toBeInTheDocument();
  });

  it("lists the possible parents in tree order, each subcategory under its own parent", async () => {
    const flat = [
      category("Aluguel", { parent_id: "cat-Despesas", level: 2 }),
      category("Despesas"),
      category("Receitas", { dre_group: "gross_revenue" }),
      category("Vendas", { parent_id: "cat-Receitas", level: 2, dre_group: "gross_revenue" }),
    ];
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : listed(flat)));
    renderAt("/companies/c1/categories");

    await screen.findByRole("table");
    const select = screen.getByLabelText("Subcategoria de");
    const options = within(select).getAllByRole("option").map((option) => option.textContent);
    expect(options).toEqual(["Nenhuma: categoria principal", "Despesas", "— Aluguel", "Receitas", "— Vendas"]);
  });

  it("shows the tree when the default categories were applied from another tab meanwhile", async () => {
    let applied = false;
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") {
        applied = true;
        return { status: 409, body: { error: "the default categories can only be applied to a company without categories" } };
      }
      return listed(applied ? tree : []);
    });
    renderAt("/companies/c1/categories");

    await userEvent.click(await screen.findByRole("button", { name: "Usar categorias padrão" }));
    expect(await screen.findByRole("table")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Usar categorias padrão" })).not.toBeInTheDocument();
  });

  it("asks a top-level category for its groups, with the usual cash flow filled in", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 201, body: category("Impostos", { dre_group: "deductions" }) };
      return listed(tree);
    });
    renderAt("/companies/c1/categories");

    await userEvent.type(await screen.findByLabelText("Nome"), "Receitas extras");
    await userEvent.selectOptions(screen.getByLabelText("Grupo no DRE"), "financial_result");
    expect(screen.getByLabelText("Grupo no fluxo de caixa")).toHaveValue("financing");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByText("Impostos cadastrada.")).toBeInTheDocument();
    expect(bodyOf(fetchMock, "POST")).toEqual({
      name: "Receitas extras",
      dre_group: "financial_result",
      cash_flow_group: "financing",
    });
  });

  it("sends only the parent for a subcategory, which inherits the groups, and starts from its button", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 201, body: category("Luz", { parent_id: "cat-Despesas", level: 2 }) };
      return listed(tree);
    });
    renderAt("/companies/c1/categories");

    await userEvent.click(await screen.findByRole("button", { name: "Nova subcategoria de Despesas" }));
    expect(screen.getByLabelText("Subcategoria de")).toHaveValue("cat-Despesas");
    expect(screen.queryByLabelText("Grupo no DRE")).not.toBeInTheDocument();
    expect(screen.getByText(/Herda de Despesas/)).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Nome"), "Luz");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByText("Luz cadastrada.")).toBeInTheDocument();
    expect(bodyOf(fetchMock, "POST")).toEqual({ name: "Luz", parent_id: "cat-Despesas" });
    // The category for transfers takes no subcategory.
    expect(screen.queryByRole("button", { name: "Nova subcategoria de Transferências" })).not.toBeInTheDocument();
  });

  it("changes only the name and the flag, and keeps the transfer category active", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") {
        return { status: 200, body: { ...tree[2], name: "Entre contas" } };
      }
      return listed(tree);
    });
    renderAt("/companies/c1/categories");

    await userEvent.click(await screen.findByRole("button", { name: "Alterar Transferências" }));
    expect(screen.getByText(/O lugar na árvore e os grupos não mudam/)).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Ativa" })).toBeDisabled();
    await userEvent.clear(screen.getByLabelText("Nome"));
    await userEvent.type(screen.getByLabelText("Nome"), "Entre contas");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));

    expect(await screen.findByText("Entre contas atualizada.")).toBeInTheDocument();
    expect(bodyOf(fetchMock, "PUT")).toEqual({ name: "Entre contas", active: true });
  });

  it("shows what the API refused, such as deactivating a parent with active subcategories", async () => {
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") {
        return { status: 409, body: { error: "deactivate the subcategories of this category first" } };
      }
      return listed(tree);
    });
    renderAt("/companies/c1/categories");

    await userEvent.click(await screen.findByRole("button", { name: "Alterar Despesas" }));
    await userEvent.click(screen.getByRole("checkbox", { name: "Ativa" }));
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("deactivate the subcategories");
  });
});

describe("bank accounts", () => {
  function account(name: string, overrides: Partial<BankAccount> = {}): BankAccount {
    return {
      id: `acc-${name}`,
      company_id: "c1",
      kind: "checking",
      name,
      bank_code: null,
      branch: null,
      number: null,
      active: true,
      ...overrides,
    };
  }

  const page = (...items: BankAccount[]) => ({ status: 200, body: { items, next_cursor: null } });
  const bodyOf = (fetchMock: ReturnType<typeof mockApi>, method: string) => {
    const call = fetchMock.mock.calls.find(([, init]) => init?.method === method);
    return JSON.parse(String(call?.[1]?.body));
  };

  it("registers an account with the bank written as typed, for the API to put in one form", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 201, body: account("Conta da loja") };
      return page();
    });
    renderAt("/companies/c1/bank-accounts");

    await userEvent.type(await screen.findByLabelText("Nome"), "Conta da loja");
    await userEvent.type(screen.getByLabelText("Código do banco"), "341");
    await userEvent.type(screen.getByLabelText("Agência"), "12.34");
    await userEvent.type(screen.getByLabelText("Conta"), "56789-0");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByText("Conta da loja cadastrada.")).toBeInTheDocument();
    expect(bodyOf(fetchMock, "POST")).toEqual({
      kind: "checking",
      name: "Conta da loja",
      bank_code: "341",
      branch: "12.34",
      number: "56789-0",
      active: true,
    });
  });

  it("takes no bank for cash and only the issuer for a card, and sends no more", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 201, body: account("Caixa", { kind: "cash" }) };
      return page();
    });
    renderAt("/companies/c1/bank-accounts");

    // A card is asked for its issuer and never for a number.
    await userEvent.selectOptions(await screen.findByLabelText("Tipo"), "card");
    expect(screen.getByLabelText("Banco emissor")).toBeInTheDocument();
    expect(screen.queryByLabelText("Agência")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Conta")).not.toBeInTheDocument();
    expect(screen.getByText(/o número do cartão nunca é salvo/)).toBeInTheDocument();

    // Cash is asked for nothing of the bank, and what was typed for another kind is gone.
    await userEvent.selectOptions(screen.getByLabelText("Tipo"), "checking");
    await userEvent.type(screen.getByLabelText("Código do banco"), "341");
    await userEvent.selectOptions(screen.getByLabelText("Tipo"), "cash");
    expect(screen.queryByLabelText("Código do banco")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Agência")).not.toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Nome"), "Caixa");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));

    expect(await screen.findByText("Caixa cadastrada.")).toBeInTheDocument();
    const sent = bodyOf(fetchMock, "POST");
    expect(sent.kind).toBe("cash");
    for (const field of ["bank_code", "branch", "number"]) {
      expect(sent).not.toHaveProperty(field);
    }
  });

  it("changes an account with its kind fixed, and shows what the API refused", async () => {
    const main = account("Conta principal", { bank_code: "341", branch: "1234", number: "56789X" });
    let answer = 409;
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") {
        return answer === 200
          ? { status: 200, body: { ...main, number: "99999" } }
          : { status: 409, body: { error: "a bank account with this bank, branch and number already exists" } };
      }
      return page(main);
    });
    renderAt("/companies/c1/bank-accounts");

    await screen.findByRole("table");
    await userEvent.click(screen.getByRole("button", { name: "Alterar Conta principal" }));
    expect(screen.getByLabelText("Tipo")).toBeDisabled();
    expect(screen.getByLabelText("Agência")).toHaveValue("1234");
    await userEvent.clear(screen.getByLabelText("Conta"));
    await userEvent.type(screen.getByLabelText("Conta"), "99999");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("already exists");

    answer = 200;
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));
    expect(await screen.findByText("Conta principal atualizada.")).toBeInTheDocument();
    expect(bodyOf(fetchMock, "PUT")).toEqual({
      kind: "checking",
      name: "Conta principal",
      bank_code: "341",
      branch: "1234",
      number: "99999",
      active: true,
    });
    expect(screen.getByText("99999")).toBeInTheDocument();
  });

  it("filters by kind, situation and text, and shows the history with the kind in words", async () => {
    const savings = account("Reserva", { kind: "savings" });
    const entry: HistoryEntry = {
      at: Date.UTC(2026, 10, 5, 12, 0),
      actor: { kind: "user", id: "u1", name: "Ana" },
      action: "created",
      changes: [{ field: "kind", from: null, to: "savings" }],
    };
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/history")) return { status: 200, body: [entry] };
      return page(savings);
    });
    renderAt("/companies/c1/bank-accounts");
    await screen.findByRole("table");

    await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "savings");
    await userEvent.selectOptions(screen.getByLabelText("Situação"), "true");
    await userEvent.type(screen.getByRole("searchbox", { name: "Buscar" }), "res");
    await waitFor(() => {
      const urls = fetchMock.mock.calls.map(([url]) => String(url));
      expect(urls).toContain("/api/companies/c1/bank-accounts?q=res&kind=savings&active=true");
 }, { timeout: 3000 });

    await userEvent.click(await screen.findByRole("button", { name: "Histórico de Reserva" }));
    const region = await screen.findByRole("region", { name: "Histórico de Reserva" });
    expect(await within(region).findByText(/Poupança/)).toBeInTheDocument();
  });
});

describe("payment methods", () => {
  function method(name: string, overrides: Partial<PaymentMethod> = {}): PaymentMethod {
    return { id: `pm-${name}`, company_id: "c1", kind: "pix", name, active: true, ...overrides };
  }

  const page = (...items: PaymentMethod[]) => ({ status: 200, body: { items, next_cursor: null } });
  const bodyOf = (fetchMock: ReturnType<typeof mockApi>, httpMethod: string) => {
    const call = fetchMock.mock.calls.find(([, init]) => init?.method === httpMethod);
    return JSON.parse(String(call?.[1]?.body));
  };

  it("registers one with its kind, changes the kind in place and filters the list", async () => {
    const visa = method("Visa", { kind: "credit_card" });
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 201, body: method("Boleto", { kind: "bank_slip" }) };
      if (init?.method === "PUT") return { status: 200, body: { ...visa, kind: "debit_card", name: "Visa débito" } };
      return page(visa);
    });
    renderAt("/companies/c1/payment-methods");
    await screen.findByRole("table");

    await userEvent.selectOptions(screen.getByLabelText("Tipo"), "bank_slip");
    await userEvent.type(screen.getByLabelText("Nome"), "Boleto");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));
    expect(await screen.findByText("Boleto cadastrada.")).toBeInTheDocument();
    expect(bodyOf(fetchMock, "POST")).toEqual({ kind: "bank_slip", name: "Boleto", active: true });
    // The list is loaded again after a registration; an edit must not race its answer.
    await waitFor(() => {
      const lists = fetchMock.mock.calls.filter(
        ([url, init]) => String(url).endsWith("/payment-methods") && init?.method === undefined,
      );
      expect(lists.length).toBeGreaterThanOrEqual(2);
    });

    await userEvent.click(screen.getByRole("button", { name: "Alterar Visa" }));
    expect(screen.getByLabelText("Tipo")).toHaveValue("credit_card");
    await userEvent.selectOptions(screen.getByLabelText("Tipo"), "debit_card");
    await userEvent.clear(screen.getByLabelText("Nome"));
    await userEvent.type(screen.getByLabelText("Nome"), "Visa débito");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));
    expect(await screen.findByText("Visa débito atualizada.")).toBeInTheDocument();
    expect(bodyOf(fetchMock, "PUT")).toEqual({ kind: "debit_card", name: "Visa débito", active: true });

    await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "pix");
    await waitFor(() => {
      const urls = fetchMock.mock.calls.map(([url]) => String(url));
      expect(urls).toContain("/api/companies/c1/payment-methods?kind=pix");
 }, { timeout: 3000 });
  });

  it("shows what the API refused, next to the form", async () => {
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 409, body: { error: "this name is already in use" } };
      return page(method("Pix"));
    });
    renderAt("/companies/c1/payment-methods");
    await screen.findByRole("table");

    await userEvent.type(screen.getByLabelText("Nome"), "pix");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("already in use");
  });
});

describe("payment terms", () => {
  function term(name: string, installments: PaymentTerm["installments"], active = true): PaymentTerm {
    return { id: `pt-${name}`, company_id: "c1", name, installments, active };
  }

  const halves = [
    { days: 0, percent: "50" },
    { days: 30, percent: "50" },
  ];
  const page = (...items: PaymentTerm[]) => ({ status: 200, body: { items, next_cursor: null } });
  const bodyOf = (fetchMock: ReturnType<typeof mockApi>, method: string) => {
    const call = fetchMock.mock.calls.find(([, init]) => init?.method === method);
    return JSON.parse(String(call?.[1]?.body));
  };

  it("lists the installments in words", async () => {
    mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: profile } : page(term("Entrada e 30", halves)),
    );
    renderAt("/companies/c1/payment-terms");

    const row = (await screen.findByText("Entrada e 30")).closest("tr")!;
    expect(row).toHaveTextContent("à vista 50% · 30 dias 50%");
  });

  it("adds up the percentages as they are typed and registers a term that closes 100", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") return { status: 201, body: term("Entrada e 30", halves) };
      return page();
    });
    renderAt("/companies/c1/payment-terms");

    // The list's own "loading" status is gone once the empty state shows.
    await screen.findByText("Nenhum prazo de pagamento ainda.");
    const submit = screen.getByRole("button", { name: "Adicionar" });
    await userEvent.type(screen.getByLabelText("Nome"), "Entrada e 30");
    // One installment of 100% to begin with; a second starts 30 days later.
    await userEvent.click(screen.getByRole("button", { name: "Adicionar parcela" }));
    expect(screen.getByLabelText("Dias da parcela 2")).toHaveValue("30");
    // The new installment has no percentage yet, and the message names it.
    expect(screen.getByRole("status")).toHaveTextContent("Preencha a porcentagem da parcela 2");

    await userEvent.clear(screen.getByLabelText("Porcentagem da parcela 1"));
    await userEvent.type(screen.getByLabelText("Porcentagem da parcela 1"), "40,5");
    await userEvent.type(screen.getByLabelText("Porcentagem da parcela 2"), "59");
    expect(screen.getByRole("status")).toHaveTextContent("Faltam 0,5% para fechar 100%.");
    expect(submit).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Porcentagem da parcela 2"), ",5");
    expect(screen.getByRole("status")).toHaveTextContent("Total: 100%.");
    expect(submit).toBeEnabled();
    await userEvent.click(submit);

    expect(await screen.findByText("Entrada e 30 cadastrado.")).toBeInTheDocument();
    expect(bodyOf(fetchMock, "POST")).toEqual({
      name: "Entrada e 30",
      installments: [
        { days: 0, percent: "40.5" },
        { days: 30, percent: "59.5" },
      ],
      active: true,
    });
  });

  it("splits equally with the remainder on the last, and says when it is over", async () => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : page()));
    renderAt("/companies/c1/payment-terms");

    await screen.findByText("Nenhum prazo de pagamento ainda.");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar parcela" }));
    await userEvent.click(screen.getByRole("button", { name: "Adicionar parcela" }));
    await userEvent.click(screen.getByRole("button", { name: "Dividir igualmente" }));
    expect(screen.getByLabelText("Porcentagem da parcela 1")).toHaveValue("33,33");
    expect(screen.getByLabelText("Porcentagem da parcela 2")).toHaveValue("33,33");
    expect(screen.getByLabelText("Porcentagem da parcela 3")).toHaveValue("33,34");
    expect(screen.getByRole("status")).toHaveTextContent("Total: 100%.");

    await userEvent.clear(screen.getByLabelText("Porcentagem da parcela 1"));
    await userEvent.type(screen.getByLabelText("Porcentagem da parcela 1"), "50");
    expect(screen.getByRole("status")).toHaveTextContent("Passou 16,67% de 100%.");
    await userEvent.click(screen.getByRole("button", { name: "Remover parcela 3" }));
    expect(screen.queryByLabelText("Dias da parcela 3")).not.toBeInTheDocument();
  });

  it("takes zeros after the second decimal and refuses a third decimal that is not zero", async () => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : page()));
    renderAt("/companies/c1/payment-terms");

    await screen.findByText("Nenhum prazo de pagamento ainda.");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar parcela" }));
    await userEvent.clear(screen.getByLabelText("Porcentagem da parcela 1"));
    await userEvent.type(screen.getByLabelText("Porcentagem da parcela 1"), "33,330");
    await userEvent.type(screen.getByLabelText("Porcentagem da parcela 2"), "66,67");
    expect(screen.getByRole("status")).toHaveTextContent("Total: 100%.");

    // 33.333 would be rounded by the database, so it is never sent as a share.
    await userEvent.clear(screen.getByLabelText("Porcentagem da parcela 1"));
    await userEvent.type(screen.getByLabelText("Porcentagem da parcela 1"), "33,333");
    expect(screen.getByRole("status")).toHaveTextContent("Preencha a porcentagem da parcela 1");
    expect(screen.getByRole("button", { name: "Adicionar" })).toBeDisabled();
  });

  it("leaves the days empty when the next one would pass the limit", async () => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : page()));
    renderAt("/companies/c1/payment-terms");

    await screen.findByText("Nenhum prazo de pagamento ainda.");
    await userEvent.clear(screen.getByLabelText("Dias da parcela 1"));
    await userEvent.type(screen.getByLabelText("Dias da parcela 1"), "3640");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar parcela" }));
    expect(screen.getByLabelText("Dias da parcela 2")).toHaveValue("");
  });

  it("changes a term with its installments filled in, and shows what the API refused", async () => {
    const entry = term("Entrada e 30", halves);
    let answer = 422;
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") {
        return answer === 200
          ? { status: 200, body: { ...entry, installments: [{ days: 0, percent: "40" }, { days: 45, percent: "60" }] } }
          : { status: 422, body: { error: "the days of the installments must increase, each later than the one before" } };
      }
      return page(entry);
    });
    renderAt("/companies/c1/payment-terms");

    await screen.findByRole("table");
    await userEvent.click(screen.getByRole("button", { name: "Alterar Entrada e 30" }));
    expect(screen.getByLabelText("Dias da parcela 2")).toHaveValue("30");
    expect(screen.getByLabelText("Porcentagem da parcela 1")).toHaveValue("50");
    await userEvent.clear(screen.getByLabelText("Porcentagem da parcela 1"));
    await userEvent.type(screen.getByLabelText("Porcentagem da parcela 1"), "40");
    await userEvent.clear(screen.getByLabelText("Porcentagem da parcela 2"));
    await userEvent.type(screen.getByLabelText("Porcentagem da parcela 2"), "60");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("must increase");

    answer = 200;
    await userEvent.clear(screen.getByLabelText("Dias da parcela 2"));
    await userEvent.type(screen.getByLabelText("Dias da parcela 2"), "45");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));
    expect(await screen.findByText("Entrada e 30 atualizado.")).toBeInTheDocument();
    const puts = fetchMock.mock.calls.filter(([, init]) => init?.method === "PUT");
    expect(puts).toHaveLength(2);
    expect(JSON.parse(String(puts[1][1]?.body))).toEqual({
      name: "Entrada e 30",
      installments: [
        { days: 0, percent: "40" },
        { days: 45, percent: "60" },
      ],
      active: true,
    });
    expect(screen.getByText("à vista 40% · 45 dias 60%")).toBeInTheDocument();
  });

  it("asks for the days of every installment, and shows the history in words", async () => {
    const entry = term("Entrada e 30", halves);
    const history: HistoryEntry = {
      at: Date.UTC(2026, 10, 5, 12, 0),
      actor: { kind: "user", id: "u1", name: "Ana" },
      action: "updated",
      changes: [{ field: "installments", from: "0:50,30:50", to: "0:40,30:60" }],
    };
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/history")) return { status: 200, body: [history] };
      return page(entry);
    });
    renderAt("/companies/c1/payment-terms");
    await screen.findByRole("table");

    await userEvent.type(screen.getByLabelText("Nome"), "Novo");
    await userEvent.clear(screen.getByLabelText("Dias da parcela 1"));
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Informe os dias de cada parcela");
    expect(fetchMock.mock.calls.some(([, init]) => init?.method === "POST")).toBe(false);

    await userEvent.click(screen.getByRole("button", { name: "Histórico de Entrada e 30" }));
    const region = await screen.findByRole("region", { name: "Histórico de Entrada e 30" });
    expect(
      await within(region).findByText(/à vista 50% · 30 dias 50% → à vista 40% · 30 dias 60%/),
    ).toBeInTheDocument();
  });
});

describe("changing a party, its roles and its address", () => {
  const withAddress: Party = {
    ...party("Padaria"),
    address: {
      postal_code: "01310100",
      street: "Avenida Paulista",
      number: "1000",
      complement: null,
      district: null,
      city: "São Paulo",
      state: "SP",
    },
  };
  const page = (...items: Party[]) => ({ status: 200, body: { items, next_cursor: null } });
  const puts = (fetchMock: ReturnType<typeof mockApi>) =>
    fetchMock.mock.calls.filter(([, init]) => init?.method === "PUT");
  const sentBody = (fetchMock: ReturnType<typeof mockApi>, index = 0) =>
    JSON.parse(String(puts(fetchMock)[index][1]?.body));

  it("sends the whole party with the new roles and address, and updates the row in place", async () => {
    const changed: Party = { ...withAddress, is_carrier: true };
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") return { status: 200, body: changed };
      return page(party("Padaria"));
    });
    renderAt("/companies/c1/parties");

    await screen.findByRole("table");
    await userEvent.click(screen.getByRole("button", { name: "Alterar Padaria" }));
    expect(screen.getByLabelText("Nome")).toHaveValue("Padaria");
    await userEvent.click(screen.getByLabelText("Transportadora"));
    await userEvent.type(screen.getByLabelText("CEP"), "01310-100");
    await userEvent.type(screen.getByLabelText("Logradouro"), "Avenida Paulista");
    await userEvent.type(screen.getByLabelText("Número"), "1000");
    await userEvent.type(screen.getByLabelText("Cidade"), "São Paulo");
    await userEvent.selectOptions(screen.getByLabelText("UF"), "SP");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));

    const row = (await screen.findByText("São Paulo/SP")).closest("tr")!;
    expect(row).toHaveTextContent("Cliente · Transportadora");
    expect(String(puts(fetchMock)[0][0])).toBe("/api/companies/c1/parties/Padaria");
    expect(sentBody(fetchMock)).toEqual({
      name: "Padaria",
      is_customer: true,
      is_supplier: false,
      is_carrier: true,
      is_seller: false,
      address: {
        postal_code: "01310-100",
        street: "Avenida Paulista",
        number: "1000",
        city: "São Paulo",
        state: "SP",
      },
      contacts: [],
      tags: [],
    });
    // The form is back to registering.
    expect(screen.getByRole("heading", { name: "Novo cliente ou fornecedor" })).toBeInTheDocument();
  });

  it("fills the form with the address and leaves it out when every part is cleared", async () => {
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") return { status: 200, body: party("Padaria") };
      return page(withAddress);
    });
    renderAt("/companies/c1/parties");

    await screen.findByRole("table");
    await userEvent.click(screen.getByRole("button", { name: "Alterar Padaria" }));
    expect(screen.getByLabelText("CEP")).toHaveValue("01310-100");
    expect(screen.getByLabelText("UF")).toHaveValue("SP");
    for (const label of ["CEP", "Logradouro", "Número", "Cidade"]) {
      await userEvent.clear(screen.getByLabelText(label));
    }
    await userEvent.selectOptions(screen.getByLabelText("UF"), "");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));

    await waitFor(() => expect(puts(fetchMock)).toHaveLength(1));
    expect(sentBody(fetchMock)).not.toHaveProperty("address");
  });

  it("shows what the API refused, such as a half-filled address, and keeps the form", async () => {
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") {
        return { status: 422, body: { error: "address street is required, or too long or has null characters" } };
      }
      return page(party("Padaria"));
    });
    renderAt("/companies/c1/parties");

    await screen.findByRole("table");
    await userEvent.click(screen.getByRole("button", { name: "Alterar Padaria" }));
    await userEvent.type(screen.getByLabelText("CEP"), "01310-100");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("address street is required");
    expect(screen.getByLabelText("CEP")).toHaveValue("01310-100");
  });

  it("tells what the registry said only when the document changed", async () => {
    const withCnpj = partyWithCheck({
      status: "verified",
      legal_name: "OFICINA LTDA",
      situation: "ATIVA",
      needs_attention: false,
    });
    let answer: Party = withCnpj;
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") return { status: 200, body: answer };
      return page(withCnpj);
    });
    renderAt("/companies/c1/parties");

    await screen.findByRole("table");
    // Same document: nothing was asked, so nothing is said.
    await userEvent.click(screen.getByRole("button", { name: "Alterar Padaria Central" }));
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));
    await screen.findByRole("heading", { name: "Novo cliente ou fornecedor" });
    expect(screen.queryByText(/CNPJ verificado/)).not.toBeInTheDocument();

    // Another document: the answer for the new one is reported, like at registration.
    answer = {
      ...withCnpj,
      document: "11222333000181",
      document_check: { status: "not_found", legal_name: null, situation: null, needs_attention: true },
    };
    await userEvent.click(screen.getByRole("button", { name: "Alterar Padaria Central" }));
    await userEvent.clear(screen.getByLabelText("CPF ou CNPJ"));
    await userEvent.type(screen.getByLabelText("CPF ou CNPJ"), "11.222.333/0001-81");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));
    expect(await screen.findByText(/Atualizado, mas a Receita não encontrou este CNPJ/)).toBeInTheDocument();
  });

  it("drops a party from a list of one kind when the change makes it another", async () => {
    const supplier: Party = { ...party("Padaria"), is_customer: false, is_supplier: true };
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") return { status: 200, body: supplier };
      return page(party("Padaria"), party("Zeca"));
    });
    renderAt("/companies/c1/parties");

    await screen.findByRole("table");
    await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "customer");
    await userEvent.click(await screen.findByRole("button", { name: "Alterar Padaria" }));
    await userEvent.click(screen.getByLabelText("Cliente"));
    await userEvent.click(screen.getByLabelText("Fornecedor"));
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));

    await waitFor(() => expect(screen.queryByRole("button", { name: "Alterar Padaria" })).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Alterar Zeca" })).toBeInTheDocument();
  });

  it("sends the contacts and tags typed, skips an empty contact row, and fills the form with what is stored", async () => {
    const stored: Party = {
      ...party("Padaria"),
      tags: ["atacado", "vip"],
      contacts: [
        { name: "Bruno", job_title: "Compras", email: "bruno@padaria.example", phone: "(81) 99999-0000" },
        { name: "Carla", job_title: null, email: null, phone: null },
      ],
    };
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") return { status: 200, body: stored };
      return page(stored);
    });
    renderAt("/companies/c1/parties");

    expect(await screen.findByRole("table")).toHaveTextContent("2 contatos");
    await userEvent.click(screen.getByRole("button", { name: "Alterar Padaria" }));
    expect(screen.getByLabelText("Etiquetas")).toHaveValue("atacado, vip");
    expect(screen.getByLabelText("Nome do contato 1")).toHaveValue("Bruno");
    expect(screen.getByLabelText("Cargo do contato 1")).toHaveValue("Compras");
    expect(screen.getByLabelText("Telefone do contato 2")).toHaveValue("");

    // Drop Carla, add Dani and a row left empty, and change the tags.
    await userEvent.click(screen.getByRole("button", { name: "Remover contato 2" }));
    await userEvent.click(screen.getByRole("button", { name: "Adicionar contato" }));
    await userEvent.type(screen.getByLabelText("Nome do contato 2"), "  Dani ");
    await userEvent.type(screen.getByLabelText("Telefone do contato 2"), "81 3333-4444");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar contato" }));
    await userEvent.clear(screen.getByLabelText("Etiquetas"));
    await userEvent.type(screen.getByLabelText("Etiquetas"), " vip, novo ,, ");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));

    await waitFor(() => expect(puts(fetchMock)).toHaveLength(1));
    const body = sentBody(fetchMock);
    expect(body.tags).toEqual(["vip", "novo"]);
    expect(body.contacts).toEqual([
      { name: "Bruno", job_title: "Compras", email: "bruno@padaria.example", phone: "(81) 99999-0000" },
      { name: "Dani", phone: "81 3333-4444" },
    ]);
  });

  it("puts the cursor in the name of a contact just added", async () => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : page(party("Padaria"))));
    renderAt("/companies/c1/parties");

    await screen.findByRole("table");
    await userEvent.click(screen.getByRole("button", { name: "Alterar Padaria" }));
    await userEvent.click(screen.getByRole("button", { name: "Adicionar contato" }));
    expect(screen.getByLabelText("Nome do contato 1")).toHaveFocus();
  });

  it("keeps a contact the API refused in the form, with the message", async () => {
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") {
        return { status: 422, body: { error: "phone must have 8 to 15 digits, written with only spaces and the characters + - ( ) ." } };
      }
      return page(party("Padaria"));
    });
    renderAt("/companies/c1/parties");

    await screen.findByRole("table");
    await userEvent.click(screen.getByRole("button", { name: "Alterar Padaria" }));
    await userEvent.click(screen.getByRole("button", { name: "Adicionar contato" }));
    await userEvent.type(screen.getByLabelText("Nome do contato 1"), "Ana");
    await userEvent.type(screen.getByLabelText("Telefone do contato 1"), "123");
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("phone must have 8 to 15 digits");
    expect(screen.getByLabelText("Telefone do contato 1")).toHaveValue("123");
  });

  it("filters the list by a tag typed, or by clicking one on a row", async () => {
    const tagged: Party = { ...party("Padaria"), tags: ["vip"] };
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      return page(tagged, party("Zeca"));
    });
    renderAt("/companies/c1/parties");

    await screen.findByRole("table");
    await userEvent.click(screen.getByRole("button", { name: "Filtrar pela etiqueta vip" }));
    expect(screen.getByLabelText("Etiqueta")).toHaveValue("vip");
    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith("/api/companies/c1/parties?tag=vip", expect.anything()),
    );

    await userEvent.clear(screen.getByLabelText("Etiqueta"));
    // A comma is never part of a tag, so it is not typed into the filter.
    await userEvent.type(screen.getByLabelText("Etiqueta"), "ata,cado");
    await waitFor(() =>
      expect(fetchMock).toHaveBeenCalledWith("/api/companies/c1/parties?tag=atacado", expect.anything()),
    );
  });

  it("drops a party from a list of one tag when the change removes the tag", async () => {
    const tagged: Party = { ...party("Padaria"), tags: ["vip"] };
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "PUT") return { status: 200, body: party("Padaria") };
      return page(tagged, { ...party("Zeca"), tags: ["vip"] });
    });
    renderAt("/companies/c1/parties");

    await screen.findByRole("table");
    await userEvent.type(screen.getByLabelText("Etiqueta"), "VIP");
    await userEvent.click(await screen.findByRole("button", { name: "Alterar Padaria" }));
    await userEvent.clear(screen.getByLabelText("Etiquetas"));
    await userEvent.click(screen.getByRole("button", { name: "Salvar alterações" }));

    await waitFor(() => expect(screen.queryByRole("button", { name: "Alterar Padaria" })).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Alterar Zeca" })).toBeInTheDocument();
  });

  it("cancels a change and goes back to registering", async () => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : page(party("Padaria"))));
    renderAt("/companies/c1/parties");

    await screen.findByRole("table");
    await userEvent.click(screen.getByRole("button", { name: "Alterar Padaria" }));
    expect(screen.getByRole("heading", { name: "Alterar Padaria" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(screen.getByRole("heading", { name: "Novo cliente ou fornecedor" })).toBeInTheDocument();
    expect(screen.getByLabelText("Nome")).toHaveValue("");
  });

  it("filters by carrier and seller, and names the new fields in the history", async () => {
    const entry: HistoryEntry = {
      at: Date.UTC(2026, 10, 5, 12, 0),
      actor: { kind: "user", id: "u1", name: "Ana" },
      action: "updated",
      changes: [
        { field: "is_carrier", from: "false", to: "true" },
        { field: "postal_code", from: null, to: "01310100" },
        { field: "street_number", from: null, to: "1000" },
        { field: "tags", from: "atacado, vip", to: "vip" },
        { field: "contacts", from: null, to: "Ana (Compras) <ana@padaria.example>" },
      ],
    };
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/history")) return { status: 200, body: [entry] };
      return page(party("Padaria"));
    });
    renderAt("/companies/c1/parties");

    await screen.findByRole("table");
    const requested = () => fetchMock.mock.calls.map(([url]) => String(url));
    await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "carrier");
    await waitFor(() => expect(requested()).toContain("/api/companies/c1/parties?kind=carrier"));
    await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "seller");
    await waitFor(() => expect(requested()).toContain("/api/companies/c1/parties?kind=seller"));

    await userEvent.click(await screen.findByRole("button", { name: "Histórico de Padaria" }));
    expect(await screen.findByText("Transportadora:")).toBeInTheDocument();
    expect(screen.getByText("não → sim")).toBeInTheDocument();
    expect(screen.getByText("CEP:")).toBeInTheDocument();
    expect(screen.getByText("Número:")).toBeInTheDocument();
    expect(screen.getByText("Etiquetas:")).toBeInTheDocument();
    expect(screen.getByText("Contatos:")).toBeInTheDocument();
    // The history uses the names people read, not the API's (the import help names the columns).
    const history = screen.getByRole("region", { name: "Histórico de Padaria" });
    expect(within(history).queryByText(/is_carrier|postal_code|street_number/)).not.toBeInTheDocument();
  });
});

describe("looking an address up by CEP", () => {
  const page = { status: 200, body: { items: [party("Padaria")], next_cursor: null } };
  const lookups = (fetchMock: ReturnType<typeof mockApi>) =>
    fetchMock.mock.calls.filter(([url]) => String(url).includes("/postal-codes/"));

  async function openTheForm() {
    renderAt("/companies/c1/parties");
    await screen.findByRole("table");
    await userEvent.click(screen.getByRole("button", { name: "Alterar Padaria" }));
  }

  it("fills the address in when asked, keeps the number, and puts the cursor on it", async () => {
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/postal-codes/")) {
        return {
          status: 200,
          body: { street: "Avenida Paulista", district: "Bela Vista", city: "São Paulo", state: "SP" },
        };
      }
      return page;
    });
    await openTheForm();

    // Nothing is asked until the person says so, and not before the CEP has eight digits.
    const button = screen.getByRole("button", { name: "Buscar endereço" });
    expect(button).toBeDisabled();
    await userEvent.type(screen.getByLabelText("CEP"), "01310-10");
    expect(button).toBeDisabled();
    await userEvent.type(screen.getByLabelText("CEP"), "0");
    expect(button).toBeEnabled();
    expect(lookups(fetchMock)).toHaveLength(0);

    await userEvent.type(screen.getByLabelText("Complemento"), "conj. 12");
    await userEvent.click(button);

    expect(await screen.findByText(/Endereço preenchido/)).toBeInTheDocument();
    expect(String(lookups(fetchMock)[0][0])).toBe("/api/companies/c1/postal-codes/01310100");
    expect(screen.getByLabelText("Logradouro")).toHaveValue("Avenida Paulista");
    expect(screen.getByLabelText("Bairro")).toHaveValue("Bela Vista");
    expect(screen.getByLabelText("Cidade")).toHaveValue("São Paulo");
    expect(screen.getByLabelText("UF")).toHaveValue("SP");
    // What only the person knows is left alone, and the cursor waits for the number.
    expect(screen.getByLabelText("Complemento")).toHaveValue("conj. 12");
    expect(screen.getByLabelText("Número")).toHaveFocus();
  });

  it("fills only the city and the state for a code without a street, and keeps what was typed", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/postal-codes/")) {
        return { status: 200, body: { street: null, district: null, city: "Cidade Pequena", state: "PE" } };
      }
      return page;
    });
    await openTheForm();

    await userEvent.type(screen.getByLabelText("Logradouro"), "Rua do Meio");
    await userEvent.type(screen.getByLabelText("CEP"), "55555000");
    await userEvent.click(screen.getByRole("button", { name: "Buscar endereço" }));

    expect(await screen.findByText(/este CEP não tem rua/)).toBeInTheDocument();
    // The code is shown tidied, as it was asked.
    expect(screen.getByLabelText("CEP")).toHaveValue("55555-000");
    expect(screen.getByLabelText("Cidade")).toHaveValue("Cidade Pequena");
    expect(screen.getByLabelText("UF")).toHaveValue("PE");
    expect(screen.getByLabelText("Logradouro")).toHaveValue("Rua do Meio");
  });

  it("ignores an answer that arrives after the CEP was changed", async () => {
    let answer: (response: Response) => void = () => {};
    const slow = new Promise<Response>((resolve) => {
      answer = resolve;
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        if (url === "/api/auth/me") return new Response(JSON.stringify(profile), { status: 200 });
        if (url.includes("/postal-codes/")) return slow;
        return new Response(JSON.stringify(page.body), { status: 200 });
      }),
    );
    await openTheForm();

    await userEvent.type(screen.getByLabelText("CEP"), "01310-100");
    await userEvent.click(screen.getByRole("button", { name: "Buscar endereço" }));
    // While the first answer is on its way, the person changes the code.
    await userEvent.clear(screen.getByLabelText("CEP"));
    await userEvent.type(screen.getByLabelText("CEP"), "50000-000");
    answer(
      new Response(
        JSON.stringify({ street: "Avenida Paulista", district: "Bela Vista", city: "São Paulo", state: "SP" }),
        { status: 200 },
      ),
    );

    await waitFor(() => expect(screen.getByRole("button", { name: "Buscar endereço" })).toBeEnabled());
    expect(screen.getByLabelText("Logradouro")).toHaveValue("");
    expect(screen.queryByText(/Endereço preenchido/)).not.toBeInTheDocument();
  });

  it("says what went wrong and leaves the fields to be typed", async () => {
    let status = 404;
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/postal-codes/")) {
        return {
          status,
          body: {
            error:
              status === 404
                ? "no address found for this postal code, or the service could not tell: check the number or fill the address in by hand"
                : "the postal code service is not answering right now: fill the address in by hand",
          },
        };
      }
      return page;
    });
    await openTheForm();

    await userEvent.type(screen.getByLabelText("CEP"), "99999-999");
    await userEvent.click(screen.getByRole("button", { name: "Buscar endereço" }));
    expect(await screen.findByText(/no address found for this postal code/)).toBeInTheDocument();

    status = 503;
    await userEvent.click(screen.getByRole("button", { name: "Buscar endereço" }));
    await waitFor(() =>
      expect(screen.getByText(/the postal code service is not answering/)).toBeInTheDocument(),
    );
    // Editing the code clears what the last lookup said about the old one.
    await userEvent.clear(screen.getByLabelText("CEP"));
    await waitFor(() =>
      expect(screen.queryByText(/the postal code service is not answering/)).not.toBeInTheDocument(),
    );
    // The form still works by hand.
    await userEvent.type(screen.getByLabelText("Logradouro"), "Rua A");
    expect(screen.getByLabelText("Logradouro")).toHaveValue("Rua A");
  });
});

describe("importing parties from a CSV file", () => {
  const empty = { status: 200, body: { items: [], next_cursor: null } };
  const csv = (text = "name,customer\nAna,sim\n") =>
    new File([text], "clientes.csv", { type: "text/csv" });
  const imports = (fetchMock: ReturnType<typeof mockApi>) =>
    fetchMock.mock.calls.filter(([url]) => String(url).includes("/parties/import"));

  async function chooseFile(file: File) {
    renderAt("/companies/c1/parties");
    await screen.findByRole("heading", { name: "Importar de um arquivo CSV" });
    await userEvent.upload(screen.getByLabelText("Arquivo CSV"), file);
  }

  it("offers a template, and needs a file before it checks or imports", async () => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : empty));
    renderAt("/companies/c1/parties");

    await screen.findByRole("heading", { name: "Importar de um arquivo CSV" });
    const template = screen.getByRole("link", { name: "Baixar modelo" });
    expect(template).toHaveAttribute("download", "clientes-e-fornecedores.csv");
    expect(decodeURIComponent(template.getAttribute("href") ?? "")).toContain("name,document,email");
    expect(screen.getByRole("button", { name: "Verificar arquivo" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Importar" })).toBeDisabled();
  });

  it("checks the file without writing, listing every problem with its row", async () => {
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/parties/import")) {
        return {
          status: 200,
          body: {
            dry_run: true,
            valid: 1,
            created: 0,
            skipped: [],
            refused: [
              { row: 3, message: "document is not a valid CPF or CNPJ" },
              { row: 4, message: "the same document as row 2" },
            ],
          },
        };
      }
      return empty;
    });
    await chooseFile(csv());
    await userEvent.click(screen.getByRole("button", { name: "Verificar arquivo" }));

    expect(await screen.findByText(/Nada foi importado: 2 linhas têm problema/)).toBeInTheDocument();
    // The input gets the keyboard back after it was emptied.
    expect(screen.getByLabelText("Arquivo CSV")).toHaveFocus();
    expect(screen.getByText("Linha 3: document is not a valid CPF or CNPJ")).toBeInTheDocument();
    expect(screen.getByText("Linha 4: the same document as row 2")).toBeInTheDocument();
    const [url, init] = imports(fetchMock)[0];
    expect(String(url)).toBe("/api/companies/c1/parties/import?dry_run=true");
    expect(init?.method).toBe("POST");
    expect(new Headers(init?.headers).get("Content-Type")).toBe("text/csv");
    // The file goes as it is, not read into text and sent again.
    expect(init?.body).toBeInstanceOf(File);
    // A file with problems is let go: the fixed one is chosen again.
    expect(screen.getByRole("button", { name: "Importar" })).toBeDisabled();
  });

  it("imports, says what was skipped, and reloads the list", async () => {
    let imported = false;
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/parties/import")) {
        imported = true;
        expect(url).not.toContain("dry_run");
        return {
          status: 200,
          body: {
            dry_run: false,
            valid: 3,
            created: 2,
            skipped: [{ row: 2, message: "a party with this document already exists" }],
            refused: [],
          },
        };
      }
      expect(init?.method).toBeUndefined();
      return imported
        ? { status: 200, body: { items: [party("Ana"), party("Bruno")], next_cursor: null } }
        : empty;
    });
    await chooseFile(csv());
    await userEvent.click(screen.getByRole("button", { name: "Importar" }));

    expect(await screen.findByText("2 cadastros criados; 1 linha ignorada.")).toBeInTheDocument();
    expect(screen.getByText("Linha 2: a party with this document already exists")).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Alterar Bruno" })).toBeInTheDocument();
    expect(imports(fetchMock)).toHaveLength(1);
    // The file is let go once it was imported, so it is not sent twice by a second click.
    expect(screen.getByRole("button", { name: "Importar" })).toBeDisabled();
  });

  it("shows why a file that cannot be read was refused", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/parties/import")) {
        return {
          status: 422,
          body: { error: "the file has columns this import does not know: phone" },
        };
      }
      return empty;
    });
    await chooseFile(csv("name,phone\nAna,1\n"));
    await userEvent.click(screen.getByRole("button", { name: "Importar" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("does not know: phone");
    // The file is let go, so the fixed one has to be chosen again.
    expect(screen.getByRole("button", { name: "Importar" })).toBeDisabled();
  });
});

describe("importing products from a CSV file", () => {
  const empty = { status: 200, body: { items: [], next_cursor: null } };

  it("offers its own template and sends the file to the products endpoint", async () => {
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/products/import")) {
        return {
          status: 200,
          body: { dry_run: true, valid: 2, created: 0, skipped: [], refused: [] },
        };
      }
      return empty;
    });
    renderAt("/companies/c1/products");

    await screen.findByRole("heading", { name: "Importar de um arquivo CSV" });
    const template = screen.getByRole("link", { name: "Baixar modelo" });
    expect(template).toHaveAttribute("download", "produtos-e-servicos.csv");
    expect(decodeURIComponent(template.getAttribute("href") ?? "")).toContain("kind,name,sku,unit");
    await userEvent.upload(
      screen.getByLabelText("Arquivo CSV"),
      new File(["name,unit\nPão,KG\n"], "produtos.csv", { type: "text/csv" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Verificar arquivo" }));

    expect(await screen.findByText(/Arquivo em ordem: 2 linhas válidas/)).toBeInTheDocument();
    const sent = fetchMock.mock.calls.filter(([url]) => String(url).includes("/products/import"));
    expect(String(sent[0][0])).toBe("/api/companies/c1/products/import?dry_run=true");
  });

  it("reloads the list after products were created and shows the skipped SKUs", async () => {
    let imported = false;
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/products/import")) {
        imported = true;
        return {
          status: 200,
          body: {
            dry_run: false,
            valid: 2,
            created: 1,
            skipped: [{ row: 3, message: "a product with this SKU already exists" }],
            refused: [],
          },
        };
      }
      return imported
        ? {
            status: 200,
            body: {
              items: [
                {
                  id: "p1",
                  company_id: "c1",
                  kind: "product",
                  name: "Pão francês",
                  sku: "PAO-001",
                  unit: "KG",
                  ncm: null,
                  sale_price: "12.5",
                  cost_price: null,
                  minimum_stock: null,
                  active: true,
                },
              ],
              next_cursor: null,
            },
          }
        : empty;
    });
    renderAt("/companies/c1/products");

    await screen.findByRole("heading", { name: "Importar de um arquivo CSV" });
    await userEvent.upload(
      screen.getByLabelText("Arquivo CSV"),
      new File(["name,unit\nPão,KG\n"], "produtos.csv", { type: "text/csv" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Importar" }));

    expect(await screen.findByText("1 cadastro criado; 1 linha ignorada.")).toBeInTheDocument();
    expect(screen.getByText("Linha 3: a product with this SKU already exists")).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Alterar Pão francês" })).toBeInTheDocument();
  });
});

describe("members and invitations", () => {
  const ana = { user_id: "u1", name: "Ana", email: "ana@example.com", role: "owner" } as const;
  const open = {
    id: "i1",
    email: "bia@example.com",
    role: "sales",
    created_at: Date.UTC(2026, 10, 5),
    expires_at: Date.UTC(2026, 10, 12),
    expired: false,
  } as const;

  const sent = (fetchMock: ReturnType<typeof mockApi>, method: string) =>
    fetchMock.mock.calls.filter(([, init]) => init?.method === method);

  it("shows the members link only to a role that reaches members", async () => {
    const viewer: Profile = {
      ...profile,
      companies: [{ id: "c1", name: "Padaria", role: "viewer", permissions: { parties: "read" } }],
    };
    mockApi((url) =>
      url === "/api/auth/me"
        ? { status: 200, body: viewer }
        : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");
    await screen.findByRole("heading", { name: "Clientes e fornecedores" });
    expect(screen.queryByRole("link", { name: "Membros" })).not.toBeInTheDocument();
  });

  it("lists who belongs and the open invitations, and says when one expired", async () => {
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/members")) return { status: 200, body: [ana] };
      return { status: 200, body: [open, { ...open, id: "i2", email: "cris@example.com", expired: true }] };
    });
    renderAt("/companies/c1/members");

    expect(await screen.findByRole("link", { name: "Membros" })).toBeInTheDocument();
    expect(await screen.findByText("ana@example.com")).toBeInTheDocument();
    expect(screen.getAllByText("Dono").length).toBeGreaterThan(0);
    expect(screen.getByText("bia@example.com")).toBeInTheDocument();
    const openInvitations = screen.getByRole("region", { name: "Convites em aberto" });
    expect(within(openInvitations).getAllByText("Vendas")).toHaveLength(2);
    expect(screen.getByText("Expirado")).toBeInTheDocument();
  });

  it("invites with the chosen role and shows the link once, saying if the e-mail did not go out", async () => {
    let invitations: unknown[] = [];
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/members")) return { status: 200, body: [ana] };
      if (init?.method === "POST") {
        invitations = [open];
        return {
          status: 201,
          body: { ...open, link: "https://erp.example/invitations/abc123", emailed: false },
        };
      }
      return { status: 200, body: invitations };
    });
    renderAt("/companies/c1/members");

    await userEvent.type(await screen.findByLabelText("E-mail"), "bia@example.com");
    await userEvent.selectOptions(screen.getByLabelText("Papel"), "sales");
    expect(screen.getByText(/Cadastra clientes e fornecedores; só consulta o resto/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Convidar" }));

    expect(await screen.findByText(/o e-mail não foi enviado/)).toBeInTheDocument();
    expect(screen.getByLabelText("Link do convite")).toHaveValue("https://erp.example/invitations/abc123");
    expect(JSON.parse(String(sent(fetchMock, "POST")[0][1]?.body))).toEqual({
      email: "bia@example.com",
      role: "sales",
    });
    // The list shows it now, and the e-mail field is empty again.
    expect(await screen.findByRole("button", { name: "Retirar o convite de bia@example.com" })).toBeInTheDocument();
    expect(screen.getByLabelText("E-mail")).toHaveValue("");
  });

  describe("changing and removing members", () => {
    const bia = { user_id: "u2", name: "Bia", email: "bia@example.com", role: "sales" } as const;

    it("offers no role or removal for the owner, and both for the others", async () => {
      mockApi((url) => {
        if (url === "/api/auth/me") return { status: 200, body: profile };
        return { status: 200, body: url.endsWith("/members") ? [ana, bia] : [] };
      });
      renderAt("/companies/c1/members");

      expect(await screen.findByRole("combobox", { name: "Papel de Bia" })).toHaveValue("sales");
      expect(screen.getByRole("button", { name: "Remover Bia" })).toBeInTheDocument();
      expect(screen.queryByRole("combobox", { name: "Papel de Ana" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Remover Ana" })).not.toBeInTheDocument();
    });

    it("does not offer an admin to change or remove themselves", async () => {
      const admin: Profile = {
        ...profile,
        companies: [{ id: "c1", name: "Padaria", role: "admin", permissions: OWNER_PERMISSIONS }],
      };
      mockApi((url) => {
        if (url === "/api/auth/me") return { status: 200, body: admin };
        return { status: 200, body: url.endsWith("/members") ? [{ ...ana, role: "admin" }, bia] : [] };
      });
      renderAt("/companies/c1/members");

      expect(await screen.findByRole("combobox", { name: "Papel de Bia" })).toBeInTheDocument();
      expect(screen.queryByRole("combobox", { name: "Papel de Ana" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Remover Ana" })).not.toBeInTheDocument();
      expect(screen.getByRole("row", { name: /ana@example.com/ })).toHaveTextContent("Administrador");
    });

    it("changes a role and shows the list again", async () => {
      let role: string = "sales";
      const fetchMock = mockApi((url, init) => {
        if (url === "/api/auth/me") return { status: 200, body: profile };
        if (init?.method === "PATCH") {
          role = JSON.parse(String(init.body)).role;
          return { status: 204 };
        }
        return { status: 200, body: url.endsWith("/members") ? [ana, { ...bia, role }] : [] };
      });
      renderAt("/companies/c1/members");

      await userEvent.selectOptions(await screen.findByRole("combobox", { name: "Papel de Bia" }), "financial");

      await waitFor(() => expect(screen.getByRole("combobox", { name: "Papel de Bia" })).toHaveValue("financial"));
      expect(String(sent(fetchMock, "PATCH")[0][0])).toBe("/api/companies/c1/members/u2");
    });

    it("asks before removing, and removes only once confirmed", async () => {
      let members = [ana, bia];
      const fetchMock = mockApi((url, init) => {
        if (url === "/api/auth/me") return { status: 200, body: profile };
        if (init?.method === "DELETE") {
          members = [ana];
          return { status: 204 };
        }
        return { status: 200, body: url.endsWith("/members") ? members : [] };
      });
      renderAt("/companies/c1/members");

      await userEvent.click(await screen.findByRole("button", { name: "Remover Bia" }));
      expect(sent(fetchMock, "DELETE")).toHaveLength(0);
      await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
      expect(screen.getByRole("button", { name: "Remover Bia" })).toBeInTheDocument();

      await userEvent.click(screen.getByRole("button", { name: "Remover Bia" }));
      await userEvent.click(screen.getByRole("button", { name: "Confirmar a remoção de Bia" }));

      await waitFor(() => expect(screen.queryByText("bia@example.com")).not.toBeInTheDocument());
      expect(String(sent(fetchMock, "DELETE")[0][0])).toBe("/api/companies/c1/members/u2");
    });

    it("says why a change was refused and reads the list again", async () => {
      let members = [ana, bia];
      mockApi((url, init) => {
        if (url === "/api/auth/me") return { status: 200, body: profile };
        if (init?.method === "PATCH") {
          members = [ana];
          return { status: 404, body: { error: "member not found" } };
        }
        return { status: 200, body: url.endsWith("/members") ? members : [] };
      });
      renderAt("/companies/c1/members");

      await userEvent.selectOptions(await screen.findByRole("combobox", { name: "Papel de Bia" }), "viewer");

      expect(await screen.findByRole("alert")).toHaveTextContent("member not found");
      expect(screen.queryByText("bia@example.com")).not.toBeInTheDocument();
    });
  });

  it("shows why an invitation was refused and keeps what was typed", async () => {
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "POST") {
        return { status: 409, body: { error: "this e-mail address already has an open invitation to the company" } };
      }
      return { status: 200, body: url.endsWith("/members") ? [ana] : [] };
    });
    renderAt("/companies/c1/members");

    await userEvent.type(await screen.findByLabelText("E-mail"), "bia@example.com");
    await userEvent.click(screen.getByRole("button", { name: "Convidar" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("already has an open invitation");
    expect(screen.getByLabelText("E-mail")).toHaveValue("bia@example.com");
  });

  it("withdraws an invitation and takes it off the list", async () => {
    let invitations: unknown[] = [open];
    const fetchMock = mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (init?.method === "DELETE") {
        invitations = [];
        return { status: 204 };
      }
      return { status: 200, body: url.endsWith("/members") ? [ana] : invitations };
    });
    renderAt("/companies/c1/members");

    await userEvent.click(await screen.findByRole("button", { name: "Retirar o convite de bia@example.com" }));

    await waitFor(() => expect(screen.getByText("Nenhum convite em aberto.")).toBeInTheDocument());
    expect(String(sent(fetchMock, "DELETE")[0][0])).toBe("/api/companies/c1/invitations/i1");
  });

  describe("the page a link leads to", () => {
    const preview = { company_name: "Mercearia", email: "bia@example.com", role: "sales", expires_at: Date.UTC(2026, 10, 12) };
    const signedOut = (url: string): Reply =>
      url === "/api/auth/me" ? { status: 401, body: { error: "authentication required" } } : { status: 200, body: preview };

    it("says what the invitation is without a session, and sends the person to sign in or sign up and come back", async () => {
      mockApi(signedOut);
      renderAt("/invitations/abc123");

      expect(await screen.findByRole("heading", { name: "Convite para Mercearia" })).toBeInTheDocument();
      expect(screen.getByText("Vendas")).toBeInTheDocument();
      expect(screen.getAllByText("bia@example.com").length).toBeGreaterThan(0);
      expect(screen.getByRole("link", { name: "Entrar" })).toHaveAttribute("href", "/login?next=%2Finvitations%2Fabc123");
      expect(screen.getByRole("link", { name: "Criar conta" })).toHaveAttribute("href", "/register?next=%2Finvitations%2Fabc123");
    });

    it("comes back to the invitation after signing in, and only to an invitation", async () => {
      const fetchMock = mockApi((url, init) => {
        if (url === "/api/auth/login") return { status: 200, body: profile };
        if (url === "/api/auth/me") return { status: 401, body: { error: "authentication required" } };
        void init;
        return { status: 200, body: preview };
      });
      renderAt("/login?next=%2Finvitations%2Fabc123");
      await userEvent.type(await screen.findByLabelText("E-mail"), "bia@example.com");
      await userEvent.type(screen.getByLabelText("Senha"), "correct horse");
      await userEvent.click(screen.getByRole("button", { name: "Entrar" }));
      expect(await screen.findByRole("heading", { name: "Convite para Mercearia" })).toBeInTheDocument();
      expect(fetchMock).toHaveBeenCalled();
    });

    it("signs up through the link with no company name, the address of the invitation, and opens the company", async () => {
      const joined = { ...profile, companies: [{ id: "c2", name: "Mercearia", role: "sales", permissions: { parties: "write" } }] };
      const fetchMock = mockApi((url) => {
        if (url === "/api/auth/me") return { status: 401, body: { error: "authentication required" } };
        if (url === "/api/auth/register") return { status: 201, body: joined };
        if (url.startsWith("/api/invitations/")) return { status: 200, body: preview };
        return { status: 200, body: { items: [], next_cursor: null } };
      });
      renderAt("/register?next=%2Finvitations%2Fabc123");

      expect(await screen.findByText("Mercearia")).toBeInTheDocument();
      expect(screen.queryByLabelText("Nome da empresa")).not.toBeInTheDocument();
      expect(screen.getByLabelText("E-mail")).toHaveValue("bia@example.com");
      expect(screen.getByLabelText("E-mail")).toHaveAttribute("readonly");
      await userEvent.type(screen.getByLabelText("Seu nome"), "Bia");
      await userEvent.type(screen.getByLabelText("Senha"), "correct horse");
      await userEvent.click(screen.getByRole("button", { name: "Criar conta" }));

      expect(await screen.findByRole("heading", { name: "Painel" })).toBeInTheDocument();
      const [, init] = fetchMock.mock.calls.find(([url]) => String(url) === "/api/auth/register")!;
      expect(JSON.parse(String(init?.body))).toEqual({
        name: "Bia",
        email: "bia@example.com",
        password: "correct horse",
        invitation: "abc123",
      });
    });

    it("falls back to an ordinary sign-up, with the reason, when the link no longer works", async () => {
      mockApi((url) =>
        url === "/api/auth/me"
          ? { status: 401, body: { error: "authentication required" } }
          : { status: 410, body: { error: "this invitation was already used or withdrawn, or has expired: ask for a new one" } },
      );
      renderAt("/register?next=%2Finvitations%2Fabc123");

      expect(await screen.findByText(/ask for a new one/)).toBeInTheDocument();
      expect(screen.getByLabelText("Nome da empresa")).toBeInTheDocument();
      expect(screen.getByLabelText("E-mail")).not.toHaveAttribute("readonly");
    });

    it("goes home, not back to the dead link, after an ordinary sign-up that began at one", async () => {
      mockApi((url) => {
        if (url === "/api/auth/me") return { status: 401, body: { error: "authentication required" } };
        if (url === "/api/auth/register") return { status: 201, body: profile };
        if (url.startsWith("/api/invitations/")) return { status: 410, body: { error: "this invitation was already used or withdrawn, or has expired: ask for a new one" } };
        return { status: 200, body: { items: [], next_cursor: null } };
      });
      renderAt("/register?next=%2Finvitations%2Fabc123");

      await userEvent.type(await screen.findByLabelText("Seu nome"), "Ana");
      await userEvent.type(screen.getByLabelText("Nome da empresa"), "Padaria");
      await userEvent.type(screen.getByLabelText("E-mail"), "ana@example.com");
      await userEvent.type(screen.getByLabelText("Senha"), "correct horse");
      await userEvent.click(screen.getByRole("button", { name: "Criar conta" }));

      expect(await screen.findByRole("heading", { name: "Painel" })).toBeInTheDocument();
      expect(screen.queryByRole("heading", { name: "Convite indisponível" })).not.toBeInTheDocument();
    });

    it("shows why a sign-up through the link was refused and keeps the form", async () => {
      mockApi((url) => {
        if (url === "/api/auth/me") return { status: 401, body: { error: "authentication required" } };
        if (url === "/api/auth/register") return { status: 409, body: { error: "an account with this e-mail address already exists" } };
        return { status: 200, body: preview };
      });
      renderAt("/register?next=%2Finvitations%2Fabc123");

      await userEvent.type(await screen.findByLabelText("Seu nome"), "Bia");
      await userEvent.type(screen.getByLabelText("Senha"), "correct horse");
      await userEvent.click(screen.getByRole("button", { name: "Criar conta" }));

      expect(await screen.findByRole("alert")).toHaveTextContent("already exists");
      expect(screen.getByLabelText("Seu nome")).toHaveValue("Bia");
    });

    it("ignores a next that is not an invitation", async () => {
      mockApi((url) =>
        url === "/api/auth/login"
          ? { status: 200, body: profile }
          : url === "/api/auth/me"
            ? { status: 401, body: { error: "authentication required" } }
            : { status: 200, body: { items: [], next_cursor: null } },
      );
      renderAt("/login?next=https%3A%2F%2Fevil.example%2F");
      await userEvent.type(await screen.findByLabelText("E-mail"), "ana@example.com");
      await userEvent.type(screen.getByLabelText("Senha"), "correct horse");
      await userEvent.click(screen.getByRole("button", { name: "Entrar" }));
      // The single company is opened, as for any sign-in: nowhere else.
      expect(await screen.findByRole("heading", { name: "Painel" })).toBeInTheDocument();
    });

    it("accepts as the signed-in user, reads the profile again and opens the company", async () => {
      let accepted = false;
      const fetchMock = mockApi((url, init) => {
        if (url === "/api/auth/me") {
          return {
            status: 200,
            body: accepted
              ? {
                  ...profile,
                  companies: [
                    ...profile.companies,
                    { id: "c2", name: "Mercearia", role: "sales", permissions: { parties: "write" } },
                  ],
                }
              : profile,
          };
        }
        if (url.endsWith("/accept") && init?.method === "POST") {
          accepted = true;
          return { status: 200, body: { company_id: "c2" } };
        }
        return { status: 200, body: preview };
      });
      renderAt("/invitations/abc123");

      await userEvent.click(await screen.findByRole("button", { name: "Aceitar convite" }));

      expect(await screen.findByRole("heading", { name: "Painel" })).toBeInTheDocument();
      expect(screen.getByText("Mercearia")).toBeInTheDocument();
      const posts = fetchMock.mock.calls.filter(([, init]) => init?.method === "POST");
      expect(String(posts[0][0])).toBe("/api/invitations/abc123/accept");
    });

    it("tells a signed-in user whose address is another one to sign in with the right one, and shows the refusal", async () => {
      mockApi((url, init) => {
        if (url === "/api/auth/me") return { status: 200, body: profile };
        if (init?.method === "POST") {
          return { status: 403, body: { error: "this invitation was sent to another e-mail address: sign in with that one" } };
        }
        return { status: 200, body: preview };
      });
      renderAt("/invitations/abc123");

      expect(await screen.findByText(/saia e entre com o e-mail do convite/)).toBeInTheDocument();
      await userEvent.click(screen.getByRole("button", { name: "Aceitar convite" }));
      expect(await screen.findByRole("alert")).toHaveTextContent("sent to another e-mail address");
    });

    it("explains a link that was used, withdrawn or has expired, and one that does not exist", async () => {
      mockApi((url) =>
        url === "/api/auth/me"
          ? { status: 200, body: profile }
          : { status: 410, body: { error: "this invitation was already used or withdrawn, or has expired: ask for a new one" } },
      );
      renderAt("/invitations/abc123");
      expect(await screen.findByRole("heading", { name: "Convite indisponível" })).toBeInTheDocument();
      expect(screen.getByText(/ask for a new one/)).toBeInTheDocument();
    });

    it("says an unknown link was not found", async () => {
      mockApi((url) =>
        url === "/api/auth/me"
          ? { status: 200, body: profile }
          : { status: 404, body: { error: "invitation not found" } },
      );
      renderAt("/invitations/nope");
      expect(await screen.findByRole("heading", { name: "Convite não encontrado" })).toBeInTheDocument();
    });
  });
});

describe("what a role that cannot write is offered", () => {
  const readOnly = { parties: "read", products: "read", finance: "read" } as const;
  const as = (permissions: Profile["companies"][number]["permissions"]): Profile => ({
    ...profile,
    companies: [{ id: "c1", name: "Padaria", role: "viewer", permissions }],
  });
  const lists = (url: string): { status: number; body: unknown } =>
    url === "/api/auth/me"
      ? { status: 200, body: null }
      : { status: 200, body: { items: [], next_cursor: null } };

  it.each([
    ["parties", "Clientes e fornecedores", "Novo cliente ou fornecedor"],
    ["products", "Produtos e serviços", "Novo produto ou serviço"],
    ["categories", "Categorias", "Nova categoria"],
    ["cost-centers", "Centros de custo", "Novo centro de custo"],
    ["bank-accounts", "Contas bancárias", "Nova conta"],
    ["payment-methods", "Formas de pagamento", "Nova forma de pagamento"],
    ["payment-terms", "Prazos de pagamento", "Novo prazo de pagamento"],
    ["entries", "Lançamentos", "Novo lançamento"],
  ])("%s: a role that only reads gets the list and no form", async (path, title, formHeading) => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: as(readOnly) } : lists(url)));
    renderAt(`/companies/c1/${path}`);

    // The page is there (its form would be too, for a role that writes) before anything is missing.
    await screen.findByRole("heading", { name: title });
    expect(screen.queryByRole("heading", { name: formHeading })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Importar/ })).not.toBeInTheDocument();
  });

  it("offers the form, the import and the edit button where the role writes", async () => {
    mockApi((url) =>
      url === "/api/auth/me"
        ? { status: 200, body: as({ ...readOnly, parties: "write" }) }
        : { status: 200, body: { items: [party("Maria")], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");

    expect(await screen.findByRole("heading", { name: "Novo cliente ou fornecedor" })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Alterar Maria" })).toBeInTheDocument();
  });

  it("tells a role that only reads that someone else makes the categories", async () => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: as(readOnly) } : lists(url)));
    renderAt("/companies/c1/categories");

    expect(await screen.findByText(/Quem cuida do financeiro/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Usar categorias padrão" })).not.toBeInTheDocument();
  });

  it("keeps the history but no edit button on a list the role only reads", async () => {
    mockApi((url) =>
      url === "/api/auth/me"
        ? { status: 200, body: as(readOnly) }
        : { status: 200, body: { items: [party("Maria")], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");

    expect(await screen.findByRole("button", { name: "Histórico de Maria" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Alterar Maria" })).not.toBeInTheDocument();
  });

  it("does not offer a read-only role to re-check a CNPJ", async () => {
    const flagged = { ...party("Maria"), document_check: { status: "unverified", legal_name: null, situation: null, needs_attention: true } };
    mockApi((url) =>
      url === "/api/auth/me"
        ? { status: 200, body: as(readOnly) }
        : { status: 200, body: { items: [flagged], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");

    expect(await screen.findByRole("button", { name: "Histórico de Maria" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Verificar de novo/ })).not.toBeInTheDocument();
  });
});

describe("how a list presents its rows", () => {
  const lists = (items: unknown[]) => (url: string) =>
    url === "/api/auth/me"
      ? { status: 200, body: profile }
      : { status: 200, body: { items, next_cursor: null } };

  it("names the action of an icon-only button in its tooltip as well as its label", async () => {
    mockApi(lists([party("Maria")]));
    renderAt("/companies/c1/parties");

    const edit = await screen.findByRole("button", { name: "Alterar Maria" });
    expect(edit).toHaveAttribute("title", "Alterar Maria");
  });

  it("says who holds a bank account and what it is, with its status as a word", async () => {
    const account: BankAccount = {
      id: "b1",
      company_id: "c1",
      kind: "checking",
      name: "Conta principal",
      bank_code: "341",
      branch: "0987",
      number: "112234",
      active: false,
    };
    mockApi(lists([account]));
    renderAt("/companies/c1/bank-accounts");

    const row = await screen.findByRole("row", { name: /Conta principal/ });
    expect(row).toHaveTextContent("Conta corrente");
    expect(row).toHaveTextContent("341");
    expect(row).toHaveTextContent("Itaú");
    expect(row).toHaveTextContent("Inativa");
  });

  it("keeps a party's roles under its name, so the table has no column for them", async () => {
    mockApi(lists([{ ...party("Maria"), is_customer: true, is_supplier: true }]));
    renderAt("/companies/c1/parties");

    const row = await screen.findByRole("row", { name: /Maria/ });
    expect(row).toHaveTextContent("Cliente · Fornecedor");
    expect(screen.queryByRole("columnheader", { name: "Tipo" })).not.toBeInTheDocument();
  });
});

describe("financial entries", () => {
  const vendas = {
    id: "cat1",
    company_id: "c1",
    parent_id: null,
    name: "Vendas",
    dre_group: "gross_revenue",
    cash_flow_group: "operating",
    level: 1,
    active: true,
  };
  const transfers = { ...vendas, id: "cat2", name: "Transferências", dre_group: "transfers", cash_flow_group: "transfer" };
  const inactive = { ...vendas, id: "cat3", name: "Antiga", active: false };
  const matriz = { id: "cc1", company_id: "c1", name: "Matriz", active: true };
  const entry: Entry = {
    id: "e1",
    company_id: "c1",
    kind: "receivable",
    description: "Venda parcelada",
    party_id: null,
    category_id: "cat1",
    cost_center_id: null,
    competence_date: "2026-11-05",
    installments: [
      { number: 1, due_date: "2026-12-10", amount: "100.5" },
      { number: 2, due_date: "2027-01-10", amount: "100.5" },
    ],
    total: "201",
  };
  const rent: Entry = {
    ...entry,
    id: "e2",
    kind: "payable",
    description: "Aluguel",
    competence_date: "2026-11-01",
    installments: [{ number: 1, due_date: "2026-11-10", amount: "2500" }],
    total: "2500",
  };

  const sent = (fetchMock: ReturnType<typeof mockApi>, method: string) =>
    fetchMock.mock.calls.filter(([, init]) => init?.method === method);

  /** The panel with the form: the sidebar is an aside too, so it is found by its heading. */
  const formPanel = async (heading: string) => {
    const panel = within((await screen.findByRole("heading", { name: heading })).closest("aside")!);
    // The form appears once what it points at has been read.
    await panel.findByLabelText("Descrição");
    return panel;
  };

  /** The registers an entry points at, and the entries given. */
  const answer =
    (entries: Entry[], extra: (url: string, init?: RequestInit) => Reply | undefined = () => undefined) =>
    (url: string, init?: RequestInit): Reply => {
      const special = extra(url, init);
      if (special) return special;
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.endsWith("/categories")) return { status: 200, body: { items: [vendas, transfers, inactive] } };
      if (url.includes("/cost-centers")) return { status: 200, body: { items: [matriz], next_cursor: null } };
      if (url.includes("/parties")) return { status: 200, body: { items: [], next_cursor: null } };
      if (url.includes("/entries")) return { status: 200, body: { items: entries, next_cursor: null } };
      return { status: 404, body: { error: "not found" } };
    };

  it("lists the entries with their kind, category, due dates and total", async () => {
    mockApi(answer([entry, rent]));
    renderAt("/companies/c1/entries");

    const row = await screen.findByRole("row", { name: /Venda parcelada/ });
    expect(row).toHaveTextContent("A receber");
    expect(row).toHaveTextContent("Vendas");
    expect(row).toHaveTextContent("05/11/2026");
    expect(row).toHaveTextContent("2 parcelas, de 10/12/2026 a 10/01/2027");
    expect(row).toHaveTextContent("R$ 201,00");
    const payable = screen.getByRole("row", { name: /Aluguel/ });
    expect(payable).toHaveTextContent("A pagar");
    expect(payable).toHaveTextContent("10/11/2026");
    expect(payable).toHaveTextContent("R$ 2.500,00");
  });

  it("registers an entry whose installments are generated from a total, the remainder on the last", async () => {
    const fetchMock = mockApi(
      answer([], (url, init) =>
        url.endsWith("/entries") && init?.method === "POST"
          ? { status: 201, body: { ...entry, id: "e9", description: "Aluguel" } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/entries");
    const form = await formPanel("Novo lançamento");

    // Only what can be used is offered: the transfers category and an inactive one are not.
    const category = form.getByLabelText("Categoria");
    expect(within(category).getAllByRole("option").map((option) => option.textContent)).toEqual([
      "Escolha uma categoria",
      "Vendas",
    ]);

    await userEvent.type(form.getByLabelText("Descrição"), "Aluguel");
    await userEvent.selectOptions(category, "cat1");
    fireEvent.change(form.getByLabelText("Competência"), { target: { value: "2026-11-01" } });
    await userEvent.type(form.getByLabelText("Valor total"), "100");
    await userEvent.clear(form.getByLabelText("Parcelas"));
    await userEvent.type(form.getByLabelText("Parcelas"), "3");
    fireEvent.change(form.getByLabelText("Primeiro vencimento"), { target: { value: "2026-12-31" } });
    await userEvent.click(form.getByRole("button", { name: "Gerar parcelas" }));

    expect(form.getByLabelText("Valor da parcela 1")).toHaveValue("33,33");
    expect(form.getByLabelText("Valor da parcela 3")).toHaveValue("33,34");
    expect(form.getByLabelText("Vencimento da parcela 2")).toHaveValue("2027-01-31");
    expect(form.getByLabelText("Vencimento da parcela 3")).toHaveValue("2027-02-28");
    expect(form.getByText("Total: R$ 100,00")).toBeInTheDocument();

    await userEvent.click(form.getByRole("button", { name: "Adicionar" }));

    await waitFor(() => expect(sent(fetchMock, "POST")).toHaveLength(1));
    expect(JSON.parse(String(sent(fetchMock, "POST")[0][1]?.body))).toEqual({
      kind: "receivable",
      description: "Aluguel",
      category_id: "cat1",
      competence_date: "2026-11-01",
      installments: [
        { due_date: "2026-12-31", amount: "33.33" },
        { due_date: "2027-01-31", amount: "33.33" },
        { due_date: "2027-02-28", amount: "33.34" },
      ],
    });
    expect(await screen.findByText("Aluguel adicionado.")).toBeInTheDocument();
  });

  it("changes an entry as a whole, with its installments", async () => {
    const fetchMock = mockApi(
      answer([entry], (url, init) =>
        url.endsWith("/entries/e1") && init?.method === "PUT"
          ? { status: 200, body: { ...entry, description: "Venda à vista", installments: [{ number: 1, due_date: "2026-12-10", amount: "201" }], total: "201" } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/entries");
    await userEvent.click(await screen.findByRole("button", { name: "Alterar Venda parcelada" }));

    const form = await formPanel("Alterar Venda parcelada");
    expect(form.getByLabelText("Descrição")).toHaveValue("Venda parcelada");
    expect(form.getByLabelText("Valor da parcela 1")).toHaveValue("100,5");
    expect(form.getByLabelText("Vencimento da parcela 2")).toHaveValue("2027-01-10");
    await userEvent.clear(form.getByLabelText("Descrição"));
    await userEvent.type(form.getByLabelText("Descrição"), "Venda à vista");
    await userEvent.click(form.getByRole("button", { name: "Remover parcela 2" }));
    await userEvent.clear(form.getByLabelText("Valor da parcela 1"));
    await userEvent.type(form.getByLabelText("Valor da parcela 1"), "201");
    await userEvent.click(form.getByRole("button", { name: "Salvar alterações" }));

    await waitFor(() => expect(sent(fetchMock, "PUT")).toHaveLength(1));
    const call = sent(fetchMock, "PUT")[0];
    expect(String(call[0])).toBe("/api/companies/c1/entries/e1");
    expect(JSON.parse(String(call[1]?.body))).toEqual({
      kind: "receivable",
      description: "Venda à vista",
      category_id: "cat1",
      competence_date: "2026-11-05",
      installments: [{ due_date: "2026-12-10", amount: "201" }],
    });
    expect(await screen.findByText("Venda à vista atualizado.")).toBeInTheDocument();
  });

  it("will not send an installment that is not a date or an amount in cents, and says which", async () => {
    const fetchMock = mockApi(answer([]));
    renderAt("/companies/c1/entries");
    const form = await formPanel("Novo lançamento");
    await userEvent.type(form.getByLabelText("Descrição"), "Conta de luz");
    await userEvent.selectOptions(form.getByLabelText("Categoria"), "cat1");
    fireEvent.change(form.getByLabelText("Competência"), { target: { value: "2026-11-01" } });

    await userEvent.click(form.getByRole("button", { name: "Adicionar" }));
    expect(await form.findByRole("alert")).toHaveTextContent("vencimento da parcela 1");

    fireEvent.change(form.getByLabelText("Vencimento da parcela 1"), { target: { value: "2026-12-10" } });
    for (const typed of ["", "1,005", "abc"]) {
      await userEvent.clear(form.getByLabelText("Valor da parcela 1"));
      if (typed) await userEvent.type(form.getByLabelText("Valor da parcela 1"), typed);
      await userEvent.click(form.getByRole("button", { name: "Adicionar" }));
      expect(await form.findByRole("alert")).toHaveTextContent("valor da parcela 1");
    }
    // `1.234` could be a thousand or a little over one: the person says which.
    await userEvent.clear(form.getByLabelText("Valor da parcela 1"));
    await userEvent.type(form.getByLabelText("Valor da parcela 1"), "1.234");
    await userEvent.click(form.getByRole("button", { name: "Adicionar" }));
    expect(await form.findByRole("alert")).toHaveTextContent("valor da parcela 1");
    expect(sent(fetchMock, "POST")).toHaveLength(0);
  });

  it("shows why the API refused an entry and keeps what was typed", async () => {
    mockApi(
      answer([], (url, init) =>
        url.endsWith("/entries") && init?.method === "POST"
          ? { status: 422, body: { error: "the category does not exist in the company, or cannot be used here" } }
          : undefined,
      ),
    );
    renderAt("/companies/c1/entries");
    const form = await formPanel("Novo lançamento");
    await userEvent.type(form.getByLabelText("Descrição"), "Conta de luz");
    await userEvent.selectOptions(form.getByLabelText("Categoria"), "cat1");
    fireEvent.change(form.getByLabelText("Competência"), { target: { value: "2026-11-01" } });
    fireEvent.change(form.getByLabelText("Vencimento da parcela 1"), { target: { value: "2026-12-10" } });
    await userEvent.type(form.getByLabelText("Valor da parcela 1"), "89,90");
    await userEvent.click(form.getByRole("button", { name: "Adicionar" }));

    expect(await form.findByRole("alert")).toHaveTextContent("cannot be used here");
    expect(form.getByLabelText("Descrição")).toHaveValue("Conta de luz");
    expect(form.getByLabelText("Valor da parcela 1")).toHaveValue("89,90");
  });

  it("narrows the list by what is typed and chosen, and starts over from the first page", async () => {
    const fetchMock = mockApi(answer([entry]));
    renderAt("/companies/c1/entries");
    await screen.findByRole("row", { name: /Venda parcelada/ });

    await userEvent.selectOptions(screen.getByLabelText("Mostrar"), "payable");
    await userEvent.selectOptions(within(screen.getByRole("search")).getByLabelText("Categoria"), "cat1");
    await waitFor(() =>
      expect(fetchMock.mock.calls.map(([url]) => String(url))).toContain(
        "/api/companies/c1/entries?kind=payable&category=cat1",
      ),
    );
  });

  it("reads the history with names, not ids", async () => {
    mockApi(
      answer([entry], (url) =>
        url.endsWith("/entries/e1/history")
          ? {
              status: 200,
              body: [
                {
                  at: Date.UTC(2026, 10, 5, 12),
                  actor: { kind: "user", id: "u1", name: "Ana" },
                  action: "created",
                  changes: [
                    { field: "category_id", from: null, to: "cat1" },
                    { field: "installments", from: null, to: "1:2026-12-10:100.5" },
                  ],
                },
              ],
            }
          : undefined,
      ),
    );
    renderAt("/companies/c1/entries");
    await userEvent.click(await screen.findByRole("button", { name: "Histórico de Venda parcelada" }));

    const history = await screen.findByRole("region", { name: "Histórico de Venda parcelada" });
    await waitFor(() => expect(history).toHaveTextContent("Categoria: Vendas"));
    expect(history).toHaveTextContent("Parcelas: 10/12/2026 R$ 100,50");
    expect(history).not.toHaveTextContent("cat1");
  });

  it("shows the entries link and keeps a role that only reads out of the form", async () => {
    mockApi(answer([entry]));
    renderAt("/companies/c1/entries");
    expect(await screen.findByRole("link", { name: "Lançamentos" })).toHaveAttribute("href", "/companies/c1/entries");
  });
});
