import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Party } from "./api";
import { mockApi, profile, renderAt } from "./testSupport";

const party = (id: string, name: string): Party => ({
  id,
  company_id: "c1",
  name,
  document: null,
  email: `${id}@x.com`,
  is_customer: true,
  is_supplier: false,
  is_carrier: false,
  is_seller: false,
  address: null,
  contacts: [],
  tags: ["vip"],
  document_check: { status: "not_applicable", legal_name: null, situation: null, needs_attention: false },
});

let saved: { name: string; text: string }[] = [];

beforeEach(() => {
  vi.restoreAllMocks();
  saved = [];
  vi.stubGlobal("URL", {
    createObjectURL: (blob: Blob) => {
      void blob.text().then((text) => (saved[saved.length - 1].text = text));
      saved.push({ name: "", text: "" });
      return "blob:x";
    },
    revokeObjectURL: () => undefined,
  });
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) {
    saved[saved.length - 1].name = this.download;
  });
});
afterEach(() => vi.unstubAllGlobals());

describe("exporting and printing a list", () => {
  it("exports every page of the list as it is filtered, not just the rows on screen", async () => {
    const fetchMock = mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("after=c2")) return { status: 200, body: { items: [party("p3", "Zélia")], next_cursor: null } };
      return { status: 200, body: { items: [party("p1", "Ana"), party("p2", "Bia; Cia")], next_cursor: "c2" } };
    });
    renderAt("/companies/c1/parties");
    await screen.findByText("Ana");

    await userEvent.click(screen.getByRole("button", { name: "Exportar" }));

    expect(await screen.findByText("3 linhas exportadas.")).toBeInTheDocument();
    await waitFor(() => expect(saved[0].text).not.toBe(""));
    expect(saved[0].name).toBe("clientes-e-fornecedores.csv");
    // Blob.text() decodes UTF-8 and drops the byte-order mark; csv.test.ts holds that it is written.
    expect(saved[0].text).toBe(
      "Nome;Documento;E-mail;Papéis;Etiquetas\r\nAna;;p1@x.com;Cliente;vip\r\n\"Bia; Cia\";;p2@x.com;Cliente;vip\r\nZélia;;p3@x.com;Cliente;vip\r\n",
    );
    expect(fetchMock.mock.calls.some(([url]) => String(url).includes("after=c2"))).toBe(true);
  });

  it("says why the file may be short when the list is longer than a file made here should be", async () => {
    let page = 0;
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      page += 1;
      const items = Array.from({ length: page === 1 ? 20 : 1000 }, (_, i) => party(`p${page}-${i}`, `Nome ${page}-${i}`));
      return { status: 200, body: { items, next_cursor: `c${page}` } };
    });
    renderAt("/companies/c1/parties");
    await screen.findByText("Nome 1-0");

    await userEvent.click(screen.getByRole("button", { name: "Exportar" }));

    expect(await screen.findByText(/primeiras 2000 linhas/)).toBeInTheDocument();
  });

  it("prints through the browser, and a person who cannot write still has both buttons", async () => {
    const print = vi.fn();
    vi.stubGlobal("print", print);
    mockApi((url) =>
      url === "/api/auth/me"
        ? {
            status: 200,
            body: { ...profile, companies: [{ ...profile.companies[0], permissions: { ...profile.companies[0].permissions, registers: "read" } }] },
          }
        : { status: 200, body: { items: [party("p1", "Ana")], next_cursor: null } },
    );
    renderAt("/companies/c1/parties");
    await screen.findByText("Ana");

    await userEvent.click(screen.getByRole("button", { name: "Imprimir" }));

    expect(print).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "Exportar" })).toBeInTheDocument();
  });
});
