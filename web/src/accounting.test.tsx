import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { HistoryEntry, PeriodLock, Profile } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const closedThroughOctober: PeriodLock = { locked_through: "2026-10-31" };

const trail: HistoryEntry[] = [
  {
    at: Date.UTC(2026, 10, 2, 15, 0),
    actor: { kind: "user", id: "u2", name: "Carla Contadora" },
    action: "created",
    changes: [{ field: "locked_through", from: null, to: "2026-10-31" }],
  },
];

/** What the API says about the lock; a PUT answers with the lock it was asked for. */
function serve(lock: PeriodLock, me: Profile = profile) {
  return mockApi((url, init) => {
    if (url === "/api/auth/me") return { status: 200, body: me };
    if (url.endsWith("/accounting/period-lock/history")) return { status: 200, body: trail };
    if (url.endsWith("/accounting/period-lock") && init?.method === "PUT") {
      return { status: 200, body: { locked_through: JSON.parse(String(init.body)).locked_through } };
    }
    if (url.endsWith("/accounting/period-lock")) return { status: 200, body: lock };
    return { status: 404, body: { error: "not found" } };
  });
}

const putsOf = (fetchMock: ReturnType<typeof mockApi>) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === "PUT").map(([, init]) => JSON.parse(String(init?.body)));

it("says through which day the books are closed and who closed them", async () => {
  serve(closedThroughOctober);
  renderAt("/companies/c1/accounting");

  expect(await screen.findByText("31/10/2026")).toBeInTheDocument();
  expect(screen.getByText(/outubro de 2026 inclusive/)).toBeInTheDocument();
  expect(await screen.findByText("Carla Contadora")).toBeInTheDocument();
});

it("offers the month after the closed one, so closing the next month is one click", async () => {
  const fetchMock = serve({ locked_through: "2020-01-31" });
  renderAt("/companies/c1/accounting");
  await screen.findByText("31/01/2020");

  expect(screen.getByLabelText("Fechar até o fim de")).toHaveValue("2020-02");
  await userEvent.click(screen.getByRole("button", { name: "Fechar período" }));

  expect(await screen.findByText("Período fechado até 29/02/2020.")).toBeInTheDocument();
  expect(putsOf(fetchMock)).toEqual([{ locked_through: "2020-02-29" }]);
  // The offer moves on with the lock.
  expect(screen.getByLabelText("Fechar até o fim de")).toHaveValue("2020-03");
});

it("closes through the last day of the month that is chosen", async () => {
  const fetchMock = serve({ locked_through: null });
  renderAt("/companies/c1/accounting");
  expect(await screen.findByText("Nenhum mês está fechado.")).toBeInTheDocument();

  fireEvent.change(screen.getByLabelText("Fechar até o fim de"), { target: { value: "2026-11" } });
  await userEvent.click(screen.getByRole("button", { name: "Fechar período" }));

  expect(await screen.findByText("Período fechado até 30/11/2026.")).toBeInTheDocument();
  expect(putsOf(fetchMock)).toEqual([{ locked_through: "2026-11-30" }]);
  expect(screen.getByText("30/11/2026")).toBeInTheDocument();
});

it("does not close a month that is closed already, and asks nothing of the server", async () => {
  const fetchMock = serve(closedThroughOctober);
  renderAt("/companies/c1/accounting");
  await screen.findByText("31/10/2026");

  fireEvent.change(screen.getByLabelText("Fechar até o fim de"), { target: { value: "2026-09" } });
  await userEvent.click(screen.getByRole("button", { name: "Fechar período" }));

  expect(await screen.findByText(/Esse mês já está fechado/)).toBeInTheDocument();
  expect(putsOf(fetchMock)).toEqual([]);
});

it("reopens only with a reason, and sends it with the new day", async () => {
  const fetchMock = serve(closedThroughOctober);
  renderAt("/companies/c1/accounting");
  await screen.findByText("31/10/2026");

  fireEvent.change(screen.getByLabelText(/Manter fechado até o fim de/), { target: { value: "2026-09" } });
  await userEvent.click(screen.getByRole("button", { name: "Reabrir período" }));
  expect(await screen.findByText(/Diga o motivo de reabrir/)).toBeInTheDocument();
  expect(putsOf(fetchMock)).toEqual([]);

  await userEvent.type(screen.getByLabelText("Motivo"), "nota fiscal chegou atrasada");
  await userEvent.click(screen.getByRole("button", { name: "Reabrir período" }));

  expect(await screen.findByText("Período reaberto: fechado até 30/09/2026.")).toBeInTheDocument();
  expect(putsOf(fetchMock)).toEqual([{ locked_through: "2026-09-30", reason: "nota fiscal chegou atrasada" }]);
});

it("shows what the server refuses, in its own words", async () => {
  mockApi((url, init) => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/history")) return { status: 200, body: [] };
    if (init?.method === "PUT") return { status: 422, body: { error: "a period is closed through the last day of a month" } };
    return { status: 200, body: { locked_through: null } };
  });
  renderAt("/companies/c1/accounting");
  await screen.findByText("Nenhum mês está fechado.");

  fireEvent.change(screen.getByLabelText("Fechar até o fim de"), { target: { value: "2026-11" } });
  await userEvent.click(screen.getByRole("button", { name: "Fechar período" }));

  expect(await screen.findByText("a period is closed through the last day of a month")).toBeInTheDocument();
});

it("lets a reader see the lock and offers no way to change it", async () => {
  const reader: Profile = {
    ...profile,
    companies: [{ id: "c1", name: "Padaria", role: "viewer", permissions: { ...OWNER_PERMISSIONS, accounting: "read" } }],
  };
  serve(closedThroughOctober, reader);
  renderAt("/companies/c1/accounting");

  expect(await screen.findByText("31/10/2026")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Fechar período" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Reabrir período" })).not.toBeInTheDocument();
});

it("is a link on the menu", async () => {
  serve({ locked_through: null });
  renderAt("/companies/c1/accounting");
  await screen.findByText("Nenhum mês está fechado.");
  expect(screen.getAllByRole("link", { name: /Contador/ }).length).toBeGreaterThan(0);
});
