import { render, screen, within } from "@testing-library/react";
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
  email_verified: true,
  companies: [
    {
      id: "c1",
      name: "Padaria",
      role: "owner",
      permissions: { parties: "write", products: "write", finance: "write", members: "write" },
    },
  ],
};

afterEach(() => {
  vi.unstubAllGlobals();
  window.localStorage.clear();
});

// A page after the menu needs a list to read: the profile answers only for the session.
function stubApi() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) =>
      url === "/api/auth/me"
        ? new Response(JSON.stringify(profile), { status: 200 })
        : new Response(JSON.stringify({ items: [], next_cursor: null }), { status: 200 }),
    ),
  );
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

  it("opens the panel of the area being looked at, with its page marked", async () => {
    stubApi();
    renderAt("/companies/c1/entries?tipo=payable");

    const panel = within(await screen.findByRole("navigation", { name: "Financeiro" }));
    expect(panel.getByRole("link", { name: "Contas a pagar" })).toHaveAttribute("aria-current", "page");
    expect(panel.getByRole("link", { name: "Fluxo de caixa" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Financeiro" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.queryByRole("link", { name: "Orçamentos" })).not.toBeInTheDocument();
  });

  it("shows the items of another area when its button is chosen, and goes back to the page's own on a move", async () => {
    stubApi();
    renderAt("/companies/c1/entries");
    await screen.findByRole("navigation", { name: "Financeiro" });

    await userEvent.click(screen.getByRole("button", { name: "Comercial" }));

    expect(screen.getByRole("navigation", { name: "Comercial" })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Financeiro" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("link", { name: "Orçamentos" }));
    expect(await screen.findByRole("navigation", { name: "Comercial" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Orçamentos" })).toHaveAttribute("aria-current", "page");
  });

  it("has direct areas for the dashboard and the reports, with nothing beside the rail", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(profile), { status: 200 })));
    renderAt("/companies/c1");

    expect(await screen.findByRole("link", { name: "Painel" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "Relatórios" })).toBeInTheDocument();
    expect(document.querySelector(".panel")).toBeNull();
  });

  it("keeps the registers of an area in a fold that can be closed", async () => {
    stubApi();
    renderAt("/companies/c1/entries");
    const panel = within(await screen.findByRole("navigation", { name: "Financeiro" }));
    const fold = panel.getByRole("button", { name: "Cadastros financeiros" });
    expect(fold).toHaveAttribute("aria-expanded", "true");
    expect(panel.getByRole("link", { name: "Contas bancárias" })).toBeInTheDocument();

    await userEvent.click(fold);

    expect(fold).toHaveAttribute("aria-expanded", "false");
    expect(panel.queryByRole("link", { name: "Contas bancárias" })).not.toBeInTheDocument();
    expect(panel.getByRole("link", { name: "Fluxo de caixa" })).toBeInTheDocument();
  });

  it("puts the panel away, shows an area's items in a flyout, and remembers the choice", async () => {
    stubApi();
    const { unmount } = renderAt("/companies/c1/entries");
    const toggle = await screen.findByRole("button", { name: "Recolher o menu" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.queryByRole("group", { name: "Financeiro" })).not.toBeInTheDocument();

    await userEvent.click(toggle);

    expect(screen.getByRole("button", { name: "Expandir o menu" })).toHaveAttribute("aria-expanded", "false");
    expect(document.querySelector(".app")).toHaveAttribute("data-collapsed");
    const flyout = within(screen.getByRole("group", { name: "Financeiro" }));
    expect(flyout.getByRole("link", { name: "Fluxo de caixa" })).toBeInTheDocument();

    unmount();
    renderAt("/companies/c1/entries");
    await userEvent.click(await screen.findByRole("button", { name: "Comercial" }));
    // Choosing an area on the shrunk menu brings the panel back, on that area.
    expect(document.querySelector(".app")).not.toHaveAttribute("data-collapsed");
    expect(screen.getByRole("navigation", { name: "Comercial" })).toBeInTheDocument();
  });

  it("offers to create only what the role may write, from any page", async () => {
    const reader = {
      ...profile,
      companies: [{ ...profile.companies[0], role: "viewer" as const, permissions: { parties: "write", finance: "read" } as const }],
    };
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(reader), { status: 200 })));
    renderAt("/companies/c1");

    await userEvent.click(await screen.findByRole("button", { name: "Novo registro" }));

    expect(screen.getByRole("menuitem", { name: "Novo cliente ou fornecedor" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Novo lançamento" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Novo produto ou serviço" })).not.toBeInTheDocument();
  });

  it("opens the phone menu onto the sections and closes it with Escape or the shade", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(profile), { status: 200 })));
    renderAt("/companies/c1");

    const menu = await screen.findByRole("button", { name: "Menu" });
    expect(menu).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(menu);
    expect(menu).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("link", { name: "Painel" })).toHaveFocus();

    await userEvent.keyboard("{Escape}");
    expect(menu).toHaveAttribute("aria-expanded", "false");
    expect(menu).toHaveFocus();

    await userEvent.click(menu);
    await userEvent.click(screen.getByRole("button", { name: "Fechar o menu" }));
    expect(menu).toHaveAttribute("aria-expanded", "false");
  });

  it("opens the phone menu on the items of the page's area, with a way back to the areas", async () => {
    stubApi();
    renderAt("/companies/c1/entries");
    await userEvent.click(await screen.findByRole("button", { name: "Menu" }));

    const shell = document.querySelector(".nav-shell");
    expect(shell).toHaveAttribute("data-level", "items");
    expect(screen.getByRole("button", { name: "Voltar às áreas" })).toHaveFocus();

    await userEvent.click(screen.getByRole("button", { name: "Voltar às áreas" }));

    expect(shell).toHaveAttribute("data-level", "areas");
    // The button that was clicked is gone from the screen: the focus goes to where the person was.
    expect(screen.getByRole("button", { name: "Financeiro" })).toHaveFocus();
    await userEvent.click(screen.getByRole("button", { name: "Cadastros" }));
    expect(shell).toHaveAttribute("data-level", "items");
    expect(screen.getByRole("button", { name: "Voltar às áreas" })).toHaveFocus();
    expect(screen.getByRole("navigation", { name: "Cadastros" })).toBeInTheDocument();
  });

  it("lets Escape put away a flyout that shows because of the focus", async () => {
    stubApi();
    renderAt("/companies/c1/entries");
    await userEvent.click(await screen.findByRole("button", { name: "Recolher o menu" }));
    const area = screen.getByRole("button", { name: "Comercial" });
    area.focus();

    await userEvent.keyboard("{Escape}");

    expect(area).not.toHaveFocus();
  });

  it("has no phone menu where there are no sections to open", async () => {
    const two = { ...profile, companies: [profile.companies[0], { ...profile.companies[0], id: "c2", name: "Mercearia" }] };
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(two), { status: 200 })));
    renderAt("/");

    expect(await screen.findByRole("heading", { name: "Escolha uma empresa" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Menu" })).not.toBeInTheDocument();
  });
});
