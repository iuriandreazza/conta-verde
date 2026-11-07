import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import { mockApi, profile, renderAt } from "./testSupport";

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

describe("email confirmation", () => {
  const unconfirmed = { ...profile, email_verified: false };

  it("asks a signed-in person to confirm the address until they do, and offers to send the link again", async () => {
    const fetchMock = mockApi((url) =>
      url === "/api/auth/me" ? { status: 200, body: unconfirmed } : url === "/api/auth/email-confirmation" ? { status: 202 } : { status: 200, body: { items: [], next_cursor: null } },
    );
    renderAt("/");

    expect(await screen.findByText("Confirme seu e-mail: enviamos um link para ana@example.com.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Reenviar link" }));
    expect(await screen.findByText("Enviamos um novo link para ana@example.com.")).toBeInTheDocument();
    expect(fetchMock.mock.calls.some(([url, init]) => String(url) === "/api/auth/email-confirmation" && init?.method === "POST")).toBe(true);
  });

  it("does not ask when the address is confirmed", async () => {
    mockApi((url) => (url === "/api/auth/me" ? { status: 200, body: profile } : { status: 200, body: { items: [], next_cursor: null } }));
    renderAt("/");
    await screen.findAllByText("Padaria");
    expect(screen.queryByText(/Confirme seu e-mail/)).not.toBeInTheDocument();
  });

  it("confirms the address with the link's token only when the person presses the button", async () => {
    const fetchMock = mockApi((url) => signedOut(url) ?? { status: 204 });
    renderAt("/confirm-email/abc123");

    const button = await screen.findByRole("button", { name: "Confirmar meu e-mail" });
    expect(fetchMock.mock.calls.some(([url]) => String(url).includes("/confirm"))).toBe(false);
    await userEvent.click(button);

    expect(await screen.findByText("E-mail confirmado. Obrigado!")).toBeInTheDocument();
    const [, init] = fetchMock.mock.calls.find(([url]) => String(url) === "/api/auth/email-confirmation/confirm")!;
    expect(JSON.parse(String(init?.body))).toEqual({ token: "abc123" });
  });

  it("says a link that is spent or expired no longer works", async () => {
    mockApi((url) => signedOut(url) ?? { status: 410, body: { error: "this link has expired or was already used" } });
    renderAt("/confirm-email/old");
    await userEvent.click(await screen.findByRole("button", { name: "Confirmar meu e-mail" }));
    expect(await screen.findByText("Este link venceu ou já foi usado.")).toBeInTheDocument();
  });
});
