import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { api, errorMessage } from "./api";
import type { Profile } from "./api";
import { AuthProvider } from "./auth";
import { OfflineBanner } from "./components/OfflineBanner";
import { CACHED_AT, FROM_CACHE } from "./pwa/offline";
import { noteAnswer, resetOfflineStatus } from "./pwa/status";

const profile: Profile = {
  user: { id: "u1", name: "Ana", email: "ana@example.com" },
  email_verified: true,
  companies: [{ id: "c1", name: "Padaria", role: "owner", permissions: { parties: "write", members: "write" } }],
};
const COMPANY = "0a1b2c3d-0000-4000-8000-000000000001";

beforeEach(() => resetOfflineStatus());
afterEach(() => {
  vi.unstubAllGlobals();
  window.localStorage.clear();
});

const answer = (headers: Record<string, string> = {}) => new Response("{}", { status: 200, headers });

describe("saying that what is shown may be old", () => {
  it("says it is a copy, from when, once the network is back too, and lifts only when the same thing is read live", () => {
    render(<OfflineBanner />);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    const at = String(Date.UTC(2027, 0, 15, 12, 0));
    act(() => noteAnswer(`/companies/${COMPANY}/parties?limit=50`, answer({ [FROM_CACHE]: "1", [CACHED_AT]: at })));
    act(() => noteAnswer(`/companies/${COMPANY}/balances?on=2027-01-15`, answer({ [FROM_CACHE]: "1", [CACHED_AT]: at })));
    expect(screen.getByRole("status")).toHaveTextContent(/uma cópia guardada neste aparelho, de 15\/01\/2027/);
    expect(screen.getByRole("status")).toHaveTextContent("A conexão voltou.");
    expect(screen.getByRole("button", { name: "Atualizar" })).toBeInTheDocument();
    // One card read live does not hide that another is still the copy.
    act(() => noteAnswer(`/companies/${COMPANY}/parties?limit=50`, answer()));
    expect(screen.getByRole("status")).toBeInTheDocument();
    // A server error is not a current answer.
    act(() => noteAnswer(`/companies/${COMPANY}/balances?on=2027-01-15`, new Response("{}", { status: 500 })));
    expect(screen.getByRole("status")).toBeInTheDocument();
    act(() => noteAnswer(`/companies/${COMPANY}/balances?on=2027-01-15`, answer()));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("ignores answers of what the worker never keeps, and says there is no connection when the device says so", () => {
    render(<OfflineBanner />);
    act(() => noteAnswer("/auth/login", answer({ [FROM_CACHE]: "1", [CACHED_AT]: "1" })));
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    act(() => resetOfflineStatus({ offline: true }));
    expect(screen.getByRole("status")).toHaveTextContent("Sem conexão.");
    expect(screen.getByRole("status")).toHaveTextContent("Não dá para salvar alterações enquanto estiver offline.");
  });

  it("a form that cannot reach the server says so in plain words, and a bug does not pretend to be a lost network", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => Promise.reject(new TypeError("Failed to fetch"))));
    const failure = await api.listConnectors("c1").catch((error: unknown) => error);
    expect(errorMessage(failure)).toMatch(/Sem conexão com o servidor.*não dá para salvar offline/);
    expect(errorMessage(new TypeError("Cannot read properties of undefined (reading 'map')"))).toBe("Algo deu errado, tente novamente.");
    expect(errorMessage(new TypeError("NetworkError when attempting to fetch resource."))).toMatch(/Sem conexão/);
    expect(errorMessage(new TypeError("Load failed"))).toMatch(/Sem conexão/);
  });
});

describe("the page that turns it on", () => {
  function renderAt(path: string) {
    return render(
      <MemoryRouter initialEntries={[path]}>
        <AuthProvider>
          <App />
        </AuthProvider>
      </MemoryRouter>,
    );
  }

  it("says what stays on the device and for how long, and turns the mode on and off", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(profile), { status: 200 })));
    vi.stubGlobal("caches", { keys: async () => [], delete: async () => true, open: async () => ({}) });
    vi.stubGlobal("indexedDB", undefined);
    Object.defineProperty(navigator, "serviceWorker", { value: {}, configurable: true });
    renderAt("/offline");
    expect(await screen.findByText(/por no\s+máximo/)).toBeInTheDocument();
    expect(screen.getByText(/dados pessoais de clientes/)).toBeInTheDocument();
    expect(screen.getByText(/Nunca ficam guardados: membros, chaves de API/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Ligar o modo offline neste aparelho" }));
    expect(await screen.findByRole("button", { name: "Desligar e apagar a cópia" })).toBeInTheDocument();
    expect(window.localStorage.getItem("conta-verde.offline.u1")).toBe("on");
    await userEvent.click(screen.getByRole("button", { name: "Desligar e apagar a cópia" }));
    await waitFor(() => expect(window.localStorage.getItem("conta-verde.offline.u1")).toBeNull());
    // Cleanup of what this test defined.
    Reflect.deleteProperty(navigator, "serviceWorker");
  });

  it("signing out clears the device's copy before the server is asked", async () => {
    const order: string[] = [];
    vi.stubGlobal("caches", { keys: async () => (order.push("clear"), ["data:u1:c1"]), delete: async () => true, open: async () => ({}) });
    vi.stubGlobal("indexedDB", undefined);
    vi.stubGlobal(
      "fetch",
      vi.fn(async (url: string) => {
        if (url === "/api/auth/logout") order.push("logout");
        return url === "/api/auth/me" ? new Response(JSON.stringify(profile), { status: 200 }) : new Response(null, { status: 204 });
      }),
    );
    renderAt("/");
    const leave = await screen.findByRole("button", { name: "Sair" });
    await waitFor(() => expect(order).toContain("clear"));
    order.length = 0;
    await userEvent.click(leave);
    await waitFor(() => expect(order).toContain("logout"));
    // The first thing that happened after pressing was the clearing, and the server was asked only after it.
    expect(order[0]).toBe("clear");
    expect(order.indexOf("clear")).toBeLessThan(order.indexOf("logout"));
  });
});
