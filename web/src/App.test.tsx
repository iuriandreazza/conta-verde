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
    expect(await screen.findByRole("heading", { name: "Sign in" })).toBeInTheDocument();
  });

  it("always links to the source code in the footer", async () => {
    mockApi(() => ({ status: 401 }));
    renderAt("/login");
    const link = await screen.findByRole("link", { name: "Source code" });
    expect(link).toHaveAttribute("href", SOURCE_URL);
  });

  it("shows server errors when signing in fails", async () => {
    mockApi((url) =>
      url === "/api/auth/me"
        ? { status: 401 }
        : { status: 401, body: { error: "invalid email or password" } },
    );
    renderAt("/login");
    await userEvent.type(await screen.findByLabelText("Email"), "ana@example.com");
    await userEvent.type(screen.getByLabelText("Password"), "wrong password");
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));
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

    await userEvent.click(screen.getByRole("button", { name: "Load more" }));
    expect(await screen.findByText("Bruno")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument();

    // The cursor contains `&` and `|`, so it must be percent-encoded.
    const requested = fetchMock.mock.calls.map(([url]) => String(url));
    expect(requested).toContain("/api/companies/c1/parties?after=id%7CCarla+%26+Filhos");
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

    await userEvent.type(await screen.findByLabelText("Name"), "Padaria Central");
    await userEvent.type(screen.getByLabelText("CPF or CNPJ"), "123");
    await userEvent.click(screen.getByRole("button", { name: "Add" }));

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("document is not a valid CPF or CNPJ"),
    );
  });
});
