/** A small filled alert mark, so an overdue state is never carried by its red alone. */
export function AlertGlyph({ size = 14 }: { size?: number }) {
  return (
    <svg className="alert-glyph" width={size} height={size} viewBox="0 0 14 14" aria-hidden>
      <circle cx={7} cy={7} r={7} fill="var(--status-critical)" />
      <path d="M7 3.5v4.2M7 10v.1" stroke="#fff" strokeWidth={1.6} strokeLinecap="round" />
    </svg>
  );
}
