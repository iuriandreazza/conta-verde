import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { HistoryEntry, PeriodLock, Profile } from "./api";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const closedThroughOctober: PeriodLock = { locked_through: "2026-10-31" };

/** What the API says about the schedule of the monthly package; a PUT answers with what it was asked. */
function scheduleReply(init: RequestInit | undefined, accountants = 2) {
  if (init?.method === "PUT") return { status: 200, body: { ...JSON.parse(String(init.body)), accountants } };
  return { status: 200, body: { enabled: false, day: 5, accountants } };
}

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
    if (url.endsWith("/accounting/package-schedule")) return scheduleReply(init);
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
  // The offer moves on with the lock, and so does the month the files are for.
  expect(screen.getByLabelText("Fechar até o fim de")).toHaveValue("2020-03");
  expect(screen.getByLabelText("Mês")).toHaveValue("2020-02");
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

describe("the accountant's files", () => {
  let saved: { name: string; text: string }[] = [];

  beforeEach(() => {
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

  /** The API of the lock, and a file answer for each export. */
  function serveFiles(answer: (url: string) => { status: number; text?: string; body?: unknown }) {
    return mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: profile };
      if (url.includes("/accounting/exports/")) return answer(url);
      if (url.endsWith("/accounting/package-schedule")) return scheduleReply(init);
      if (url.endsWith("/accounting/period-lock/history")) return { status: 200, body: trail };
      if (url.endsWith("/accounting/period-lock") && init?.method !== "PUT") return { status: 200, body: closedThroughOctober };
      return { status: 404, body: { error: "not found" } };
    });
  }

  it("asks for the month that was closed last and saves what the server makes", async () => {
    const fetchMock = serveFiles(() => ({ status: 200, text: "entry_id;kind\r\n" }));
    renderAt("/companies/c1/accounting");
    await screen.findByText("31/10/2026");

    expect(screen.getByLabelText("Mês")).toHaveValue("2026-10");
    await userEvent.click(screen.getByRole("button", { name: "Lançamentos" }));

    await waitFor(() => expect(saved[0]?.text).toBe("entry_id;kind\r\n"));
    expect(saved[0].name).toBe("entries-2026-10-01-a-2026-10-31.csv");
    expect(fetchMock.mock.calls.map(([url]) => String(url))).toContain(
      "/api/companies/c1/accounting/exports/entries?from=2026-10-01&to=2026-10-31&format=csv",
    );
  });

  it("asks for the spreadsheet and the whole package of another month", async () => {
    const fetchMock = serveFiles(() => ({ status: 200, text: "x" }));
    renderAt("/companies/c1/accounting");
    await screen.findByText("31/10/2026");

    fireEvent.change(screen.getByLabelText("Mês"), { target: { value: "2026-02" } });
    await userEvent.selectOptions(screen.getByLabelText("Formato"), "xlsx");
    await userEvent.click(screen.getByRole("button", { name: "Pacote completo (.zip)" }));

    await waitFor(() => expect(saved[0]?.name).toBe("package-2026-02-01-a-2026-02-28.zip"));
    expect(fetchMock.mock.calls.map(([url]) => String(url))).toContain(
      "/api/companies/c1/accounting/exports/package?from=2026-02-01&to=2026-02-28&format=xlsx",
    );
  });

  it("tells a refusal in the server's words and saves nothing", async () => {
    serveFiles(() => ({
      status: 422,
      body: { error: "too much for one export (more than 50,000 settlements): choose a shorter period" },
    }));
    renderAt("/companies/c1/accounting");
    await screen.findByText("31/10/2026");

    await userEvent.click(screen.getByRole("button", { name: "Baixas" }));

    expect(await screen.findByText(/too much for one export/)).toBeInTheDocument();
    expect(saved).toEqual([]);
    // The button is free again.
    expect(screen.getByRole("button", { name: "Baixas" })).toBeEnabled();
  });

  it("says plainly what the package does not hold, and lets a reader download too", async () => {
    const reader: Profile = {
      ...profile,
      companies: [{ id: "c1", name: "Padaria", role: "viewer", permissions: { ...OWNER_PERMISSIONS, accounting: "read" } }],
    };
    mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: reader };
      if (url.includes("/accounting/exports/")) return { status: 200, text: "x" };
      if (url.endsWith("/accounting/package-schedule")) return { status: 200, body: { enabled: true, day: 10, accountants: 1 } };
      if (url.endsWith("/history")) return { status: 200, body: trail };
      if (init?.method === "PUT") return { status: 403, body: { error: "forbidden" } };
      return { status: 200, body: closedThroughOctober };
    });
    renderAt("/companies/c1/accounting");
    await screen.findByText("31/10/2026");

    expect(screen.getByText(/não guarda o XML das notas de compra/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Transferências" }));
    await waitFor(() => expect(saved[0]?.name).toBe("transfers-2026-10-01-a-2026-10-31.csv"));
  });
});

describe("the monthly package notice", () => {
  /** The lock and the schedule; `accountants` is how many members the notice would reach. */
  function serveSchedule(accountants: number, me: Profile = profile) {
    return mockApi((url, init) => {
      if (url === "/api/auth/me") return { status: 200, body: me };
      if (url.endsWith("/accounting/package-schedule")) return scheduleReply(init, accountants);
      if (url.endsWith("/history")) return { status: 200, body: trail };
      return { status: 200, body: closedThroughOctober };
    });
  }

  it("says how many accountants it reaches and what it sends", async () => {
    serveSchedule(2);
    renderAt("/companies/c1/accounting");

    expect(await screen.findByText("O aviso vai para 2 contadores.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Sobre Aviso mensal aos contadores/ }));
    expect(await screen.findByRole("tooltip")).toHaveTextContent(/não traz dados da empresa nem anexos/);
  });

  it("says that nothing is sent when there is no accountant, and where to invite one", async () => {
    serveSchedule(0);
    renderAt("/companies/c1/accounting");

    expect(await screen.findByText(/Nenhum contador foi convidado: nada será enviado/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Convidar um contador" })).toHaveAttribute("href", "/companies/c1/members");
  });

  it("switches it on from a day and says so", async () => {
    const fetchMock = serveSchedule(1);
    renderAt("/companies/c1/accounting");
    await screen.findByText("O aviso vai para 1 contador.");

    await userEvent.click(screen.getByRole("checkbox", { name: "Avisar todo mês" }));
    await userEvent.selectOptions(screen.getByLabelText("A partir do dia"), "12");
    await userEvent.click(screen.getByRole("button", { name: "Salvar aviso" }));

    expect(await screen.findByText("Aviso ligado: todo dia 12.")).toBeInTheDocument();
    const puts = fetchMock.mock.calls.filter(([url, init]) => String(url).endsWith("/package-schedule") && init?.method === "PUT");
    expect(puts.map(([, init]) => JSON.parse(String(init?.body)))).toEqual([{ enabled: true, day: 12 }]);
  });

  it("shows a reader what is set and offers no way to change it", async () => {
    const reader: Profile = {
      ...profile,
      companies: [{ id: "c1", name: "Padaria", role: "viewer", permissions: { ...OWNER_PERMISSIONS, accounting: "read" } }],
    };
    mockApi((url) => {
      if (url === "/api/auth/me") return { status: 200, body: reader };
      if (url.endsWith("/accounting/package-schedule")) return { status: 200, body: { enabled: true, day: 10, accountants: 1 } };
      if (url.endsWith("/history")) return { status: 200, body: trail };
      return { status: 200, body: closedThroughOctober };
    });
    renderAt("/companies/c1/accounting");

    expect(await screen.findByText("Ligado: todo mês, a partir do dia 10.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Salvar aviso" })).not.toBeInTheDocument();
  });

  it("opens the downloads on the month the e-mail links to", async () => {
    serveSchedule(1);
    renderAt("/companies/c1/accounting?month=2026-02");
    await screen.findByText("31/10/2026");

    expect(screen.getByLabelText("Mês")).toHaveValue("2026-02");
  });
});

it("is a link on the menu", async () => {
  serve({ locked_through: null });
  renderAt("/companies/c1/accounting");
  await screen.findByText("Nenhum mês está fechado.");
  expect(screen.getAllByRole("link", { name: /Contador/ }).length).toBeGreaterThan(0);
});
