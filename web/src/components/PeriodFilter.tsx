import { useState } from "react";
import { isIsoDate } from "../lib/entryInstallments";
import { CUSTOM, PRESETS, presetOf, shiftSpan, type Span } from "../lib/period";
import { Button } from "./Button";

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
  // A date input passes through years like 0002 while the year is typed: such a span is not stepped.
  const valid = isIsoDate(value.from) && isIsoDate(value.to) && value.from >= "1000-01-01" && value.from <= value.to;
  // Two presets can be the same span (this quarter and "this month and the next two" in January,
  // April, July and October): what the person picked stays shown while it still matches.
  const [picked, setPicked] = useState<string | null>(null);
  const pickedPreset = PRESETS.find((preset) => preset.key === picked);
  const stillPicked = valid && pickedPreset !== undefined && presetOf(value, today, [pickedPreset]) === picked;
  const current = !valid ? CUSTOM : stillPicked ? picked : presetOf(value, today);
  return (
    <div className="period-filter" role="group" aria-label="Filtro de período">
      <Button
        type="button"
        variant="ghost"
        className="period-step"
        aria-label="Período anterior"
        disabled={!valid}
        onClick={() => onChange(shiftSpan(value, -1))}
      >
        <span aria-hidden>‹</span>
      </Button>
      <label>
        Período
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
              {preset.label}
            </option>
          ))}
          <option value={CUSTOM}>Personalizado</option>
        </select>
      </label>
      <Button
        type="button"
        variant="ghost"
        className="period-step"
        aria-label="Próximo período"
        disabled={!valid}
        onClick={() => onChange(shiftSpan(value, 1))}
      >
        <span aria-hidden>›</span>
      </Button>
      <label>
        De
        <input type="date" value={value.from} onChange={(event) => onChange({ ...value, from: event.target.value })} />
      </label>
      <label>
        Até
        <input type="date" value={value.to} onChange={(event) => onChange({ ...value, to: event.target.value })} />
      </label>
    </div>
  );
}
