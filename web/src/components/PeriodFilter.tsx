import { useState } from "react";
import { useT } from "../i18n";
import { isIsoDate } from "../lib/entryInstallments";
import { CUSTOM, PRESETS, presetOf, shiftSpan, type Span } from "../lib/period";
import { Button } from "./Button";
import { ChevronLeftIcon } from "./icons/chevron-left";
import { ChevronRightIcon } from "./icons/chevron-right";

interface PeriodFilterProps {
  value: Span;
  onChange: (span: Span) => void;
  /** Today where the person is, as `YYYY-MM-DD`: the presets are counted from it. */
  today: string;
}

/**
 * The period of a report: step back or forward, pick a usual one, or type the first and last day. One
 * control for every page that asks for a span, so the same question is answered the same way.
 */
export function PeriodFilter({ value, onChange, today }: PeriodFilterProps) {
  const t = useT();
  // A date input passes through years like 0002 while the year is typed: such a span is not stepped.
  const valid = isIsoDate(value.from) && isIsoDate(value.to) && value.from >= "1000-01-01" && value.from <= value.to;
  // Two presets can be the same span (this quarter and "this month and the next two" in January,
  // April, July and October): what the person picked stays shown while it still matches.
  const [picked, setPicked] = useState<string | null>(null);
  const pickedPreset = PRESETS.find((preset) => preset.key === picked);
  const stillPicked = valid && pickedPreset !== undefined && presetOf(value, today, [pickedPreset]) === picked;
  const current = !valid ? CUSTOM : stillPicked ? picked : presetOf(value, today);
  return (
    <div className="period-filter" role="group" aria-label={t("Period filter")}>
      <Button
        type="button"
        variant="ghost"
        iconOnly
        icon={ChevronLeftIcon}
        className="period-step"
        aria-label={t("Previous period")}
        disabled={!valid}
        onClick={() => onChange(shiftSpan(value, -1))}
      >
        {t("Previous period")}
      </Button>
      <label>
        {t("Period")}
        <select
          value={current}
          onChange={(event) => {
            const preset = PRESETS.find((item) => item.key === event.target.value);
            setPicked(event.target.value);
            if (preset) onChange(preset.span(today));
          }}
        >
          {PRESETS.map((preset) => (
            <option key={preset.key} value={preset.key}>
              {t(preset.label)}
            </option>
          ))}
          <option value={CUSTOM}>{t("Custom")}</option>
        </select>
      </label>
      <Button
        type="button"
        variant="ghost"
        iconOnly
        icon={ChevronRightIcon}
        className="period-step"
        aria-label={t("Next period")}
        disabled={!valid}
        onClick={() => onChange(shiftSpan(value, 1))}
      >
        {t("Next period")}
      </Button>
      <label>
        {t("From")}
        <input type="date" value={value.from} onChange={(event) => onChange({ ...value, from: event.target.value })} />
      </label>
      <label>
        {t("To")}
        <input type="date" value={value.to} onChange={(event) => onChange({ ...value, to: event.target.value })} />
      </label>
    </div>
  );
}
