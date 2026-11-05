import { formatDecimal, toApiDecimal } from "./decimal";

// Percentages are kept in hundredths of a percent (an integer), never in a `Number` with decimals:
// 33.33 + 33.33 + 33.34 is exactly 100 and a float would say 99.99999999999999.
const WHOLE = 10_000;

/**
 * `count` equal shares of 100%, in the form the API takes (`"33.33"`), with the remainder on the
 * last one, the rule of ADR 0003: 33.33, 33.33, 33.34.
 */
export function splitEvenly(count: number): string[] {
  if (count < 1) return [];
  const base = Math.floor(WHOLE / count);
  const shares = Array<number>(count).fill(base);
  shares[count - 1] = WHOLE - base * (count - 1);
  return shares.map(hundredthsToText);
}

function hundredthsToText(hundredths: number): string {
  const whole = Math.floor(hundredths / 100);
  const fraction = String(hundredths % 100).padStart(2, "0");
  return fraction === "00" ? String(whole) : `${whole}.${fraction.replace(/0$/, "")}`;
}

/**
 * What a person typed (`33,33`) in hundredths of a percent, or `null` when it is not a share.
 * Zeros after the second decimal (`33,330`) are the same number and the API accepts them.
 */
function toHundredths(typed: string): number | null {
  const text = toApiDecimal(typed);
  const match = text === undefined ? null : /^(\d{1,3})(?:\.(\d{1,2})0*)?$/.exec(text);
  if (!match) return null;
  return Number(match[1]) * 100 + Number((match[2] ?? "").padEnd(2, "0"));
}

/** The position (from 0) of the first percentage that is not a valid share, or `null`. */
export function firstInvalidShare(typed: string[]): number | null {
  const index = typed.findIndex((text) => toHundredths(text) === null);
  return index === -1 ? null : index;
}

/** The sum of the percentages typed, in hundredths, or `null` while one is not a valid share. */
export function sumOfPercents(typed: string[]): number | null {
  let total = 0;
  for (const text of typed) {
    const hundredths = toHundredths(text);
    if (hundredths === null) return null;
    total += hundredths;
  }
  return total;
}

export const WHOLE_IN_HUNDREDTHS = WHOLE;

/** `10000` as `100`, `9999` as `99,99`: how a difference is told to a person. */
export function showHundredths(hundredths: number): string {
  return formatDecimal(hundredthsToText(Math.abs(hundredths)));
}

/** "à vista" for day 0, "30 dias" for the others. */
export function dueLabel(days: number): string {
  return days === 0 ? "à vista" : `${days} ${days === 1 ? "dia" : "dias"}`;
}

/** The installments as a short sentence: `à vista 50% · 30 dias 50%`. */
export function describeInstallments(installments: { days: number; percent: string }[]): string {
  return installments
    .map((installment) => `${dueLabel(installment.days)} ${formatDecimal(installment.percent)}%`)
    .join(" · ");
}

/** The audit trail's canonical text (`0:50,30:50`) as the same sentence. */
export function describeInstallmentsText(text: string): string {
  return describeInstallments(
    text.split(",").map((part) => {
      const [days, percent] = part.split(":");
      return { days: Number(days), percent };
    }),
  );
}
