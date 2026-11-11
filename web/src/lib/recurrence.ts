import type { RecurrenceFrequency } from "../api";
import { msg, type Translate } from "../i18n";

export const FREQUENCIES: { key: RecurrenceFrequency; label: string }[] = [
  { key: "weekly", label: msg("Every week") },
  { key: "monthly", label: msg("Every month") },
  { key: "yearly", label: msg("Every year") },
];

export const frequencyLabel = (frequency: RecurrenceFrequency, t: Translate) => {
  const label = FREQUENCIES.find((option) => option.key === frequency)?.label;
  return label === undefined ? frequency : t(label);
};
