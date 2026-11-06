import { describe, expect, it } from "vitest";
import type { Entry, EntryInstallment } from "../api";
import { balance, entryStatus, installmentStatus, todayIso } from "./settlement";

const installment = (due: string, amount: string, settled: string, number = 1): EntryInstallment => ({
  number,
  due_date: due,
  amount,
  settled,
});

const entryOf = (installments: EntryInstallment[]): Entry => ({
  id: "e1",
  company_id: "c1",
  kind: "receivable",
  description: "Venda",
  party_id: null,
  category_id: "cat",
  cost_center_id: null,
  competence_date: "2026-11-01",
  installments,
  total: "0",
  recurrence_id: null,
  sale_id: null,
  sale_return_id: null,
  voided: false,
});

describe("what is left of an installment", () => {
  it("is its amount less what was settled, exactly", () => {
    expect(balance(installment("2026-12-10", "100.5", "40.25"))).toBe("60.25");
    expect(balance(installment("2026-12-10", "100", "0"))).toBe("100");
    expect(balance(installment("2026-12-10", "0.3", "0.1"))).toBe("0.2");
    expect(balance(installment("2026-12-10", "100", "100"))).toBe("0");
  });
});

describe("the status of an installment", () => {
  const today = "2026-12-10";
  it("is settled when nothing is left, whatever the date", () => {
    expect(installmentStatus(installment("2026-01-01", "100", "100"), today)).toBe("settled");
  });
  it("is overdue when something is left and it fell due before today, not on the day itself", () => {
    expect(installmentStatus(installment("2026-12-09", "100", "0"), today)).toBe("overdue");
    expect(installmentStatus(installment("2026-12-09", "100", "99.99"), today)).toBe("overdue");
    expect(installmentStatus(installment("2026-12-10", "100", "0"), today)).toBe("open");
  });
  it("is partial when some was settled and it is not late, open when nothing was", () => {
    expect(installmentStatus(installment("2026-12-31", "100", "10"), today)).toBe("partial");
    expect(installmentStatus(installment("2026-12-31", "100", "0"), today)).toBe("open");
  });
});

describe("the status of an entry", () => {
  const today = "2026-12-10";
  it("is settled only when every installment is", () => {
    expect(entryStatus(entryOf([installment("2026-11-10", "50", "50"), installment("2026-12-10", "50", "50", 2)]), today)).toBe("settled");
  });
  it("is overdue when any installment is, even if others are paid", () => {
    expect(entryStatus(entryOf([installment("2026-11-10", "50", "0"), installment("2027-01-10", "50", "0", 2)]), today)).toBe("overdue");
    expect(entryStatus(entryOf([installment("2026-11-10", "50", "50"), installment("2026-12-01", "50", "0", 2)]), today)).toBe("overdue");
  });
  it("is partial when some is paid and the rest is not late, open when nothing is paid", () => {
    expect(entryStatus(entryOf([installment("2026-11-10", "50", "50"), installment("2027-01-10", "50", "0", 2)]), today)).toBe("partial");
    expect(entryStatus(entryOf([installment("2027-01-10", "50", "0")]), today)).toBe("open");
    expect(entryStatus(entryOf([installment("2027-01-10", "50", "10")]), today)).toBe("partial");
  });
});

describe("today", () => {
  it("is the day where the person is, not in UTC", () => {
    expect(todayIso(new Date(2026, 11, 9, 23, 30))).toBe("2026-12-09");
    expect(todayIso(new Date(2026, 0, 5, 0, 5))).toBe("2026-01-05");
  });
});
