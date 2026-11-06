import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import type { Span } from "../lib/period";
import { PeriodFilter } from "./PeriodFilter";

function Harness({ start }: { start: Span }) {
  const [span, setSpan] = useState(start);
  return <PeriodFilter value={span} onChange={setSpan} today="2026-11-06" />;
}

const dates = () => [screen.getByLabelText("De"), screen.getByLabelText("Até")];

describe("PeriodFilter", () => {
  it("steps by whole months and keeps the dates the person can read", async () => {
    render(<Harness start={{ from: "2026-11-01", to: "2026-11-30" }} />);
    expect(screen.getByLabelText("Período")).toHaveValue("this-month");

    await userEvent.click(screen.getByRole("button", { name: "Próximo período" }));
    expect(dates()[0]).toHaveValue("2026-12-01");
    expect(dates()[1]).toHaveValue("2026-12-31");
    expect(screen.getByLabelText("Período")).toHaveValue("next-month");

    await userEvent.click(screen.getByRole("button", { name: "Período anterior" }));
    await userEvent.click(screen.getByRole("button", { name: "Período anterior" }));
    expect(dates()[0]).toHaveValue("2026-10-01");
    expect(screen.getByLabelText("Período")).toHaveValue("last-month");
  });

  it("applies a usual period and shows 'Personalizado' once a date is changed by hand", async () => {
    render(<Harness start={{ from: "2026-11-01", to: "2026-11-30" }} />);

    await userEvent.selectOptions(screen.getByLabelText("Período"), "year");
    expect(dates()[0]).toHaveValue("2026-01-01");
    expect(dates()[1]).toHaveValue("2026-12-31");

    await userEvent.clear(dates()[1]);
    await userEvent.type(dates()[1], "2026-06-30");
    expect(screen.getByLabelText("Período")).toHaveValue("custom");
  });

  it("calls a valid span that matches no usual period 'Personalizado' and keeps what was picked when two periods coincide", async () => {
    // In October this month and the next two is the same span as this quarter.
    function October() {
      const [span, setSpan] = useState<Span>({ from: "2026-10-01", to: "2026-10-31" });
      return <PeriodFilter value={span} onChange={setSpan} today="2026-10-06" />;
    }
    render(<October />);

    await userEvent.selectOptions(screen.getByLabelText("Período"), "quarter");
    expect(dates()[1]).toHaveValue("2026-12-31");
    expect(screen.getByLabelText("Período")).toHaveValue("quarter");

    await userEvent.selectOptions(screen.getByLabelText("Período"), "this-month");
    await userEvent.selectOptions(screen.getByLabelText("Período"), "three-months");
    expect(screen.getByLabelText("Período")).toHaveValue("three-months");

    await userEvent.click(screen.getByRole("button", { name: "Próximo período" }));
    expect(dates()[0]).toHaveValue("2027-01-01");
    expect(screen.getByLabelText("Período")).toHaveValue("custom");
  });

  it("does not step from a year still being typed", async () => {
    render(<Harness start={{ from: "0002-11-01", to: "2026-11-30" }} />);

    expect(screen.getByRole("button", { name: "Próximo período" })).toBeDisabled();
  });

  it("does not step from a span that is not a span yet", async () => {
    render(<Harness start={{ from: "", to: "2026-11-30" }} />);

    expect(screen.getByRole("button", { name: "Próximo período" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Período anterior" })).toBeDisabled();
  });
});
