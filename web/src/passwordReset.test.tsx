import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import { mockApi, renderAt } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const signedOut = (url: string) => (url === "/api/auth/me" ? { status: 401, body: { error: "authentication required" } } : undefined);

describe("password reset", () => {
  it("leads from the sign-in page to a form that asks for the link and says the same for any address", async () => {
    const fetchMock = mockApi((url) => signedOut(url) ?? { status: 202 });
    renderAt("/login");

    await userEvent.click(await screen.findByRole("link", { name: "Esqueci minha senha" }));
    await userEvent.type(await screen.findByLabelText("E-mail"), "ana@example.com");
    await userEvent.click(screen.getByRole("button", { name: "Enviar link" }));

    expect(await screen.findByText(/Se ana@example.com tiver uma conta, enviamos um link/)).toBeInTheDocument();
    const [, init] = fetchMock.mock.calls.find(([url]) => String(url) === "/api/auth/password-reset")!;
    expect(JSON.parse(String(init?.body))).toEqual({ email: "ana@example.com" });
  });

  it("chooses a new password with the link's token and sends the person to sign in", async () => {
    const fetchMock = mockApi((url) => signedOut(url) ?? { status: 204 });
    renderAt("/reset-password/abc123");

    await userEvent.type(await screen.findByLabelText("Nova senha"), "battery staple");
    await userEvent.click(screen.getByRole("button", { name: "Salvar nova senha" }));

    expect(await screen.findByText("Senha alterada. Entre com a nova senha.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Entrar" })).toHaveAttribute("href", "/login");
    const [, init] = fetchMock.mock.calls.find(([url]) => String(url) === "/api/auth/password-reset/confirm")!;
    expect(JSON.parse(String(init?.body))).toEqual({ token: "abc123", password: "battery staple" });
  });

  it("says a link that is spent or expired no longer works and offers a new one", async () => {
    mockApi((url) => signedOut(url) ?? { status: 410, body: { error: "this link has expired or was already used" } });
    renderAt("/reset-password/old");

    await userEvent.type(await screen.findByLabelText("Nova senha"), "battery staple");
    await userEvent.click(screen.getByRole("button", { name: "Salvar nova senha" }));

    expect(await screen.findByText("this link has expired or was already used")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Pedir um novo link" })).toHaveAttribute("href", "/forgot-password");
  });
});
