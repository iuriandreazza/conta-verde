import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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
  });
});
