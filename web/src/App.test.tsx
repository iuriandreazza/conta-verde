import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, vi } from "vitest";
import { App } from "./App";
import type { Party, Profile } from "./api";
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
  };
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
