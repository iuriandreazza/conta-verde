import { describe, expect, it } from "vitest";
import { lastDayOfMonth, monthLabel } from "./closing";

describe("closing a month", () => {
  it("finds the last day of any month, leap years included", () => {
    expect(lastDayOfMonth("2026-10")).toBe("2026-10-31");
    expect(lastDayOfMonth("2026-11")).toBe("2026-11-30");
    expect(lastDayOfMonth("2026-02")).toBe("2026-02-28");
    expect(lastDayOfMonth("2028-02")).toBe("2028-02-29");
    expect(lastDayOfMonth("2026-12")).toBe("2026-12-31");
  });

  it("names the month of a day", () => {
    expect(monthLabel("2026-10-31")).toBe("outubro de 2026");
  });
});
