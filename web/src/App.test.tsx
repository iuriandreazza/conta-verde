import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, vi } from "vitest";
import { App } from "./App";
import type { DocumentCheck, HistoryEntry, Party, Product, Profile } from "./api";
import { AuthProvider } from "./auth";
import { SOURCE_URL } from "./components/Footer";

const profile: Profile = {
  user: { id: "u1", name: "Ana", email: "ana@example.com" },
  companies: [{ id: "c1", name: "Padaria" }],
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

  it("opens the only company's parties and loads more with the cursor", async () => {
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("after=")) {
        return { status: 200, body: { items: [party("Bruno")], next_cursor: null } };
      }
      return { status: 200, body: { items: [party("Carla")], next_cursor: "id|Carla & Filhos" } };
    });
    renderAt("/");

    expect(await screen.findByRole("heading", { name: "Padaria" })).toBeInTheDocument();
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

  it("signs up and lands on the new company's parties", async () => {
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

    expect(await screen.findByRole("heading", { name: "Padaria" })).toBeInTheDocument();
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
                { id: "c1", name: "Padaria" },
                { id: "c2", name: "Mercearia" },
              ],
            },
          }
        : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/");

    expect(await screen.findByRole("heading", { name: "Escolha uma empresa" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("link", { name: "Mercearia" }));
    expect(await screen.findByRole("heading", { name: "Mercearia" })).toBeInTheDocument();
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
