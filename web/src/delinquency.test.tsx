import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { Delinquency } from "./api";
import { mockApi, profile, renderAt } from "./testSupport";

beforeEach(() => vi.restoreAllMocks());
afterEach(() => vi.unstubAllGlobals());

const report: Delinquency = {
  on: "2027-01-31",
  owed: "152.5",
  installments: 5,
  buckets: [
    { from_days: 1, to_days: 30, owed: "112", installments: 3 },
    { from_days: 31, to_days: 60, owed: "30.5", installments: 1 },
    { from_days: 61, to_days: 90, owed: "0", installments: 0 },
    { from_days: 91, to_days: null, owed: "10", installments: 1 },
  ],
  customers: [
    { party_id: "p1", owed: "140.5", installments: 3, oldest_due: "2026-10-01", by_bucket: ["100", "30.5", "0", "10"] },
    { party_id: null, owed: "7", installments: 1, oldest_due: "2027-01-01", by_bucket: ["7", "0", "0", "0"] },
  ],
};

function serve(body: Delinquency) {
  return mockApi((url) => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.includes("/delinquency?on=")) return { status: 200, body };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [] } };
    if (url.includes("/parties")) {
      return { status: 200, body: { items: [{ id: "p1", name: "Mercado Boa Compra" }], next_cursor: null } };
    }
    return { status: 200, body: { items: [], next_cursor: null } };
  });
}

it("shows the total and each range, and who owes, biggest first, with a row for what has no customer", async () => {
  serve(report);
  renderAt("/companies/c1/delinquency");

  const ranges = within(await screen.findByLabelText("Por faixa de atraso"));
  expect(ranges.getByText("R$ 152,50")).toBeInTheDocument();
  expect(ranges.getByText("1 a 30 dias")).toBeInTheDocument();
  expect(ranges.getByText("91 dias ou mais")).toBeInTheDocument();
  expect(ranges.getByText("R$ 112,00")).toBeInTheDocument();

  const rows = await screen.findAllByRole("row");
  const body = rows.slice(1);
  expect(body[0]).toHaveTextContent("Mercado Boa Compra");
  expect(body[0]).toHaveTextContent("R$ 140,50");
  expect(body[0]).toHaveTextContent("01/10/2026");
  expect(body[0]).toHaveTextContent("R$ 30,50");
  expect(body[1]).toHaveTextContent("Sem cliente");
  expect(body[1]).toHaveTextContent("R$ 7,00");
});

it("asks again for the day that is chosen, and says when nothing is late", async () => {
  const fetchMock = serve({ ...report, owed: "0", installments: 0, customers: [] });
  renderAt("/companies/c1/delinquency");

  expect(await screen.findByText("Nada em atraso nesta data.")).toBeInTheDocument();
  await userEvent.clear(screen.getByLabelText("Posição em"));
  await userEvent.type(screen.getByLabelText("Posição em"), "2027-01-31");
  expect(
    fetchMock.mock.calls.some(([url]) => String(url).endsWith("/delinquency?on=2027-01-31")),
  ).toBe(true);
});

it("calls a customer that is not among the names read by its id, and drops the report when the day is cleared", async () => {
  mockApi((url) => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.includes("/delinquency?on=")) return { status: 200, body: report };
    if (url.endsWith("/categories")) return { status: 200, body: { items: [] } };
    return { status: 200, body: { items: [], next_cursor: null } };
  });
  renderAt("/companies/c1/delinquency");

  expect(await screen.findByText("Cliente p1")).toBeInTheDocument();
  await userEvent.clear(screen.getByLabelText("Posição em"));
  expect(screen.queryByText("Cliente p1")).not.toBeInTheDocument();
});
