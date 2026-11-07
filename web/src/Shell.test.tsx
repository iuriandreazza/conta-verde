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

  it("groups the sections and folds a group away", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(profile), { status: 200 })));
    renderAt("/companies/c1");

    const finance = await screen.findByRole("button", { name: "Financeiro" });
    expect(screen.getByRole("link", { name: "Fluxo de caixa" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Contas bancárias" })).toBeInTheDocument();

    await userEvent.click(finance);

    expect(finance).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("link", { name: "Fluxo de caixa" })).not.toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Contas bancárias" })).toBeInTheDocument();
  });

  it("shrinks the menu to a rail of icons and expands it again, remembering the choice", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(profile), { status: 200 })));
    const { unmount } = renderAt("/companies/c1");
    const toggle = await screen.findByRole("button", { name: "Recolher o menu" });
    expect(toggle).toHaveAttribute("aria-expanded", "true");

    await userEvent.click(toggle);

    expect(screen.getByRole("button", { name: "Expandir o menu" })).toHaveAttribute("aria-expanded", "false");
    expect(document.querySelector(".app")).toHaveAttribute("data-collapsed");
    // The names stay for a screen reader and show as a tooltip where the text is out of sight.
    expect(screen.getByRole("link", { name: "Fluxo de caixa" })).toHaveAttribute("title", "Fluxo de caixa");

    unmount();
    renderAt("/companies/c1");
    expect(await screen.findByRole("button", { name: "Expandir o menu" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Expandir o menu" }));
    expect(screen.getByRole("link", { name: "Fluxo de caixa" })).not.toHaveAttribute("title");
  });

  it("shows every icon in the rail even when a group was folded before", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(profile), { status: 200 })));
    renderAt("/companies/c1");
    await userEvent.click(await screen.findByRole("button", { name: "Financeiro" }));
    expect(screen.queryByRole("link", { name: "Fluxo de caixa" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Recolher o menu" }));

    expect(screen.getByRole("link", { name: "Fluxo de caixa" })).toBeInTheDocument();
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
    expect(screen.queryByRole("button", { name: "Fechar o menu" })).not.toBeInTheDocument();

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

  it("has no phone menu where there are no sections to open", async () => {
    const two = { ...profile, companies: [profile.companies[0], { ...profile.companies[0], id: "c2", name: "Mercearia" }] };
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify(two), { status: 200 })));
    renderAt("/");

    expect(await screen.findByRole("heading", { name: "Escolha uma empresa" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Menu" })).not.toBeInTheDocument();
  });
});
