import { render } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { vi } from "vitest";
import { App } from "./App";
import type { Profile } from "./api";
import { AuthProvider } from "./auth";

export const OWNER_PERMISSIONS = {
  parties: "write",
  products: "write",
  finance: "write",
  sales: "write",
  members: "write",
} as const;

export const profile: Profile = {
  user: { id: "u1", name: "Ana", email: "ana@example.com" },
  companies: [{ id: "c1", name: "Padaria", role: "owner", permissions: OWNER_PERMISSIONS }],
};

export type Reply = { status: number; body?: unknown; text?: string };
export type Handler = (url: string, init?: RequestInit) => Reply;

/** Stubs `fetch` with the API's answers, and keeps the calls so a test can read what was sent. */
export function mockApi(handler: Handler) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const { status, body, text } = handler(String(input), init);
    return new Response(text ?? (body === undefined ? null : JSON.stringify(body)), { status });
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

export function renderAt(path: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <AuthProvider>
        <App />
      </AuthProvider>
    </MemoryRouter>,
  );
}
