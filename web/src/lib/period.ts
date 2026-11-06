export interface Span {
  from: string;
  to: string;
}

const two = (value: number) => String(value).padStart(2, "0");

const iso = (date: Date) => `${date.getUTCFullYear()}-${two(date.getUTCMonth() + 1)}-${two(date.getUTCDate())}`;

const parse = (day: string): [number, number, number] => {
  const [year, month, date] = day.split("-").map(Number);
  return [year, month, date];
};

const DAY = 86_400_000;

const utcOf = (day: string) => {
  const [year, month, date] = parse(day);
  return Date.UTC(year, month - 1, date);
};

/** `count` whole calendar months starting at `month` (1 to 12, and any number past it rolls into the next year). */
export function monthsSpan(year: number, month: number, count: number): Span {
  const first = new Date(Date.UTC(year, month - 1, 1));
  // Day 0 of the month after the last one is its last day.
  const last = new Date(Date.UTC(year, month - 1 + count, 0));
  return { from: iso(first), to: iso(last) };
}

export interface Preset {
  key: string;
  label: string;
  span: (today: string) => Span;
}

export const PRESETS: Preset[] = [
  { key: "this-month", label: "Este mês", span: (today) => monthsSpan(parse(today)[0], parse(today)[1], 1) },
  { key: "last-month", label: "Mês passado", span: (today) => monthsSpan(parse(today)[0], parse(today)[1] - 1, 1) },
  { key: "next-month", label: "Próximo mês", span: (today) => monthsSpan(parse(today)[0], parse(today)[1] + 1, 1) },
  { key: "three-months", label: "Este mês e os 2 seguintes", span: (today) => monthsSpan(parse(today)[0], parse(today)[1], 3) },
  {
    key: "quarter",
    label: "Este trimestre",
    span: (today) => monthsSpan(parse(today)[0], Math.floor((parse(today)[1] - 1) / 3) * 3 + 1, 3),
  },
  { key: "year", label: "Este ano", span: (today) => monthsSpan(parse(today)[0], 1, 12) },
];

export const CUSTOM = "custom";

/** The preset a span is exactly, or `custom` for any other span. */
export function presetOf(span: Span, today: string): string {
  return PRESETS.find((preset) => {
    const wanted = preset.span(today);
    return wanted.from === span.from && wanted.to === span.to;
  })?.key ?? CUSTOM;
}

/**
 * The span before (-1) or after (+1) this one: the same number of whole months when the span is made
 * of whole months, otherwise the same number of days.
 */
export function shiftSpan(span: Span, direction: -1 | 1): Span {
  const [fromYear, fromMonth, fromDay] = parse(span.from);
  const [toYear, toMonth] = parse(span.to);
  const months = (toYear - fromYear) * 12 + toMonth - fromMonth + 1;
  const wholeMonths = fromDay === 1 && monthsSpan(fromYear, fromMonth, months).to === span.to;
  if (wholeMonths) {
    return monthsSpan(fromYear, fromMonth + direction * months, months);
  }
  const start = utcOf(span.from);
  const days = (utcOf(span.to) - start) / DAY + 1;
  return { from: iso(new Date(start + direction * days * DAY)), to: iso(new Date(start + (direction * days + days - 1) * DAY)) };
}
