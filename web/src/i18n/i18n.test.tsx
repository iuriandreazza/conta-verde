import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { profile, mockApi, renderAt } from "../testSupport";
import { translate } from ".";
import { ptBR } from "./pt-BR";

/** The page is replaced when the session arrives, so a heading is waited for until it stays. */
const headingShows = (name: string) => waitFor(() => expect(screen.getByRole("heading", { name })).toBeInTheDocument());

const signedIn = () => mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : { status: 404, body: {} }));
const anonymous = () => mockApi((url) => (url === "/api/auth/me" ? { status: 401, body: { error: "no" } } : { status: 404, body: {} }));

/** The English texts of the catalogue that read differently in Portuguese and long enough not to occur by chance. */
const englishKeys = Object.entries(ptBR)
  .filter(([key, text]) => key !== text && key.length > 14 && !key.includes("{"))
  .map(([key]) => key);

function rawKeysOnScreen(): string[] {
  const text = document.body.textContent ?? "";
  return englishKeys.filter((key) => text.includes(key));
}

describe("translating", () => {
  it("fills the parts of a text, and shows what the catalogue lacks as it was written", () => {
    expect(translate("en", "Hello {name}", { name: "Ana" })).toBe("Hello Ana");
    expect(translate("pt-BR", "We sent a new link to {email}.", { email: "a@b.c" })).toBe("Enviamos um novo link para a@b.c.");
    expect(translate("pt-BR", "A text nobody translated yet")).toBe("A text nobody translated yet");
    expect(translate("en", "Page not found")).toBe("Page not found");
  });
});

describe("the language of the screens", () => {
  it("is Portuguese until somebody chooses, and the choice is remembered for the person", async () => {
    signedIn();
    const first = renderAt("/no-such-page");
    await headingShows("Página não encontrada");
    expect(document.documentElement.lang).toBe("pt-BR");
    expect(rawKeysOnScreen()).toEqual([]);

    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Idioma" }), "en");
    await headingShows("Page not found");
    expect(screen.getByRole("combobox", { name: "Language" })).toHaveValue("en");
    expect(document.documentElement.lang).toBe("en");
    expect(window.localStorage.getItem("conta-verde.language.u1")).toBe("en");
    first.unmount();

    // A new visit finds it again.
    renderAt("/no-such-page");
    await headingShows("Page not found");
  });

  it("is each person's own: another person's choice on the same device is theirs, and one who chose nothing gets the last", async () => {
    window.localStorage.setItem("conta-verde.language", "en");
    window.localStorage.setItem("conta-verde.language.u1", "pt-BR");
    signedIn();
    renderAt("/no-such-page");
    // The signed-in person chose Portuguese, whatever was last chosen here.
    await headingShows("Página não encontrada");
  });

  it("falls back to the language last chosen on the device for a person who never chose, and to Portuguese when storage is blocked", async () => {
    window.localStorage.setItem("conta-verde.language", "en");
    signedIn();
    renderAt("/no-such-page");
    await headingShows("Page not found");
  });

  it("reaches the pages before sign-in, in both languages, with no key shown as it was written", async () => {
    anonymous();
    const view = renderAt("/login");
    await headingShows("Entrar");
    expect(screen.getByText("Gestão simples para o seu negócio crescer.")).toBeInTheDocument();
    expect(rawKeysOnScreen()).toEqual([]);
    view.unmount();

    window.localStorage.setItem("conta-verde.language", "en");
    renderAt("/login");
    await headingShows("Sign in");
    expect(screen.getByText("Simple management for your business to grow.")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("link", { name: "Create account" })).toBeInTheDocument());
  });
});
