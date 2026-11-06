import type { RecurrenceFrequency } from "../api";

export const FREQUENCIES: { key: RecurrenceFrequency; label: string }[] = [
  { key: "weekly", label: "Toda semana" },
  { key: "monthly", label: "Todo mês" },
  { key: "yearly", label: "Todo ano" },
];

export const frequencyLabel = (frequency: RecurrenceFrequency) =>
  FREQUENCIES.find((option) => option.key === frequency)?.label ?? frequency;
