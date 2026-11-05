// Everything the dashboard shows comes from this module, and none of it is real: phase 2 replaces
// this one import with the financial endpoints (ADR 0020). Amounts are decimal strings, as the API
// will send them (ADR 0003); `sampleData.test.ts` keeps the hand-written totals consistent.

export const PERIODS = [
  { months: 3, label: "3 meses" },
  { months: 6, label: "6 meses" },
  { months: 12, label: "12 meses" },
] as const;

export type PeriodMonths = (typeof PERIODS)[number]["months"];

export interface MonthFlow {
  label: string;
  inflow: string;
  outflow: string;
}

/** The last twelve closed months, oldest first. */
export const MONTHLY_FLOW: readonly MonthFlow[] = [
  { label: "nov/25", inflow: "39800.00", outflow: "35900.00" },
  { label: "dez/25", inflow: "41200.00", outflow: "36850.40" },
  { label: "jan/26", inflow: "38950.50", outflow: "35120.00" },
  { label: "fev/26", inflow: "36400.00", outflow: "33980.75" },
  { label: "mar/26", inflow: "42800.00", outflow: "37210.30" },
  { label: "abr/26", inflow: "44150.25", outflow: "38900.00" },
  { label: "mai/26", inflow: "46900.00", outflow: "40125.60" },
  { label: "jun/26", inflow: "45300.00", outflow: "41870.00" },
  { label: "jul/26", inflow: "49750.00", outflow: "42300.45" },
  { label: "ago/26", inflow: "51200.00", outflow: "44010.00" },
  { label: "set/26", inflow: "50480.00", outflow: "45230.90" },
  { label: "out/26", inflow: "54300.00", outflow: "46780.00" },
];

export interface PeriodSummary {
  inflow: string;
  outflow: string;
  result: string;
  /** Written out, since the sample has no earlier months for the longest period. */
  change: { text: string; direction: "up" | "down" } | null;
}

export const PERIOD_SUMMARY: Record<PeriodMonths, PeriodSummary> = {
  3: {
    inflow: "155980.00",
    outflow: "136020.90",
    result: "19959.10",
    change: { text: "13,1% mais que nos 3 meses anteriores", direction: "up" },
  },
  6: {
    inflow: "297930.00",
    outflow: "260316.95",
    result: "37613.05",
    change: { text: "48,4% mais que nos 6 meses anteriores", direction: "up" },
  },
  12: { inflow: "541230.75", outflow: "478278.40", result: "62952.35", change: null },
};

export const CASH_BALANCE = "48720.35";
/** Month-end balances, oldest first, only to draw the trend line. */
export const BALANCE_TREND = [
  "31200.00", "33850.40", "33100.10", "35640.00", "36980.65", "39410.00",
  "41150.90", "43300.00", "44870.35", "46120.00", "47905.60", "48720.35",
];

export interface DueBucket {
  label: string;
  overdue: boolean;
  receivable: string;
  payable: string;
}

export const DUE_BUCKETS: readonly DueBucket[] = [
  { label: "Vencidas", overdue: true, receivable: "8420.00", payable: "3150.75" },
  { label: "Até 7 dias", overdue: false, receivable: "12900.00", payable: "9870.40" },
  { label: "8 a 30 dias", overdue: false, receivable: "21350.50", payable: "15420.00" },
  { label: "31 a 60 dias", overdue: false, receivable: "9800.00", payable: "11230.90" },
  { label: "Mais de 60 dias", overdue: false, receivable: "4100.00", payable: "2800.00" },
];

/** Receivable and payable due within 30 days (the buckets up to "8 a 30 dias"). */
export const NEXT_30_DAYS = { receivable: "34250.50", payable: "25290.40" };
export const OVERDUE_RECEIVABLE = { amount: "8420.00", count: 3 };

export interface UpcomingItem {
  id: string;
  party: string;
  description: string;
  due: string;
  amount: string;
  direction: "receivable" | "payable";
  status: "overdue" | "today" | "upcoming";
}

export const UPCOMING: readonly UpcomingItem[] = [
  { id: "u1", party: "Padaria Estrela", description: "Pedido 1042", due: "02/11", amount: "3200.00", direction: "receivable", status: "overdue" },
  { id: "u2", party: "Distribuidora Sul", description: "Nota 8891", due: "05/11", amount: "4850.40", direction: "payable", status: "today" },
  { id: "u3", party: "Mercado Boa Compra", description: "Pedido 1051", due: "07/11", amount: "7120.00", direction: "receivable", status: "upcoming" },
  { id: "u4", party: "Energia Elétrica", description: "Fatura de outubro", due: "10/11", amount: "1312.55", direction: "payable", status: "upcoming" },
  { id: "u5", party: "Clínica Vida", description: "Serviço de entrega", due: "12/11", amount: "2480.00", direction: "receivable", status: "upcoming" },
  { id: "u6", party: "Embalagens Norte", description: "Nota 3320", due: "15/11", amount: "2960.00", direction: "payable", status: "upcoming" },
];
