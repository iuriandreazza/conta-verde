import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { Profile } from "./api";
import { AuthProvider } from "./auth";
import { PageHeader } from "./components/PageHeader";

const profile: Profile = {
  user: { id: "u1", name: "Ana", email: "ana@example.com" },
  companies: [
    {
      id: "c1",
      name: "Padaria",
      role: "owner",
      permissions: { parties: "write", products: "write", finance: "write", members: "write" },
    },
  ],
};

afterEach(() => vi.unstubAllGlobals());

describe("page shell", () => {
  it("names the tab after the page and takes the name back when the page goes away", () => {
    const { unmount } = render(
      <MemoryRouter>
        <PageHeader title="Lançamentos" />
      </MemoryRouter>,
    );
    expect(document.title).toBe("Lançamentos · conta-verde");

    unmount();
    expect(document.title).toBe("conta-verde");
  });

  it("follows the person from one page to the next", async () => {
    function Pages() {
      const [page, setPage] = useState("Lançamentos");
      return (
        <>
          <button onClick={() => setPage("Relatórios")}>Next</button>
          <PageHeader key={page} title={page} />
        </>
      );
    }
    render(
      <MemoryRouter>
        <Pages />
      </MemoryRouter>,
    );
    expect(document.title).toBe("Lançamentos · conta-verde");

    await userEvent.click(screen.getByRole("button", { name: "Next" }));

    expect(document.title).toBe("Relatórios · conta-verde");
  });

  it("lets a keyboard user skip the sections and land on the content", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(profile), { status: 200 })));
    render(
      <MemoryRouter initialEntries={["/companies/c1"]}>
        <AuthProvider>
          <App />
        </AuthProvider>
      </MemoryRouter>,
    );

    const skip = await screen.findByRole("link", { name: "Ir para o conteúdo" });
    await userEvent.click(skip);

    expect(screen.getByRole("main")).toHaveFocus();
    expect(document.title).toBe("Painel · conta-verde");
    expect(window.location.hash).toBe("");
  });

  it("has nothing to skip, and still names the tab, when signed out", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(null, { status: 401 })));
    render(
      <MemoryRouter initialEntries={["/login"]}>
        <AuthProvider>
          <App />
        </AuthProvider>
      </MemoryRouter>,
    );

    expect(await screen.findByRole("heading", { name: "Entrar" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Ir para o conteúdo" })).not.toBeInTheDocument();
    expect(document.title).toBe("Entrar · conta-verde");
  });
});
