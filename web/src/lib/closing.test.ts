import { describe, expect, it } from "vitest";
import { lastDayOfMonth, monthLabel, monthToExport, suggestedMonthToClose } from "./closing";

describe("the month to close next", () => {
  it("is the month after the one closed through, once that month is over", () => {
    expect(suggestedMonthToClose("2026-09-30", "2026-11-09")).toBe("2026-10");
    // Across the turn of the year.
    expect(suggestedMonthToClose("2026-12-31", "2027-02-03")).toBe("2027-01");
  });

  it("is the last month that is over when nothing is closed yet", () => {
    expect(suggestedMonthToClose(null, "2026-11-09")).toBe("2026-10");
    expect(suggestedMonthToClose(null, "2027-01-02")).toBe("2026-12");
  });

  it("suggests nothing while the month after the lock is still running or in the future", () => {
    expect(suggestedMonthToClose("2026-10-31", "2026-11-09")).toBeUndefined();
    expect(suggestedMonthToClose("2026-12-31", "2026-11-09")).toBeUndefined();
  });
});

describe("the month to download", () => {
  it("is the month closed last, whatever today is", () => {
    expect(monthToExport("2026-10-31", "2026-11-09")).toBe("2026-10");
    expect(monthToExport("2026-03-31", "2027-02-03")).toBe("2026-03");
  });

  it("is the last month that is over when nothing is closed", () => {
    expect(monthToExport(null, "2026-11-09")).toBe("2026-10");
    expect(monthToExport(null, "2027-01-02")).toBe("2026-12");
  });
});

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
