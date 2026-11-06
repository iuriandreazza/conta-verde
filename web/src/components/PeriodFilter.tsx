import { isIsoDate } from "../lib/entryInstallments";
import { CUSTOM, PRESETS, presetOf, shiftSpan, type Span } from "../lib/period";

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
  const valid = isIsoDate(value.from) && isIsoDate(value.to) && value.from <= value.to;
  const current = valid ? presetOf(value, today) : CUSTOM;
  return (
    <div className="period-filter" role="group" aria-label="Filtro de período">
      <button
        type="button"
        className="period-step"
        aria-label="Período anterior"
        disabled={!valid}
        onClick={() => onChange(shiftSpan(value, -1))}
      >
        <span aria-hidden>‹</span>
      </button>
      <label>
        Período
        <select
          value={current}
          onChange={(event) => {
            const preset = PRESETS.find((item) => item.key === event.target.value);
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
      <button
        type="button"
        className="period-step"
        aria-label="Próximo período"
        disabled={!valid}
        onClick={() => onChange(shiftSpan(value, 1))}
      >
        <span aria-hidden>›</span>
      </button>
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
