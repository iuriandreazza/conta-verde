export interface LegendItem {
  name: string;
  color: string;
  /** A forecast is drawn dashed, so it differs from what happened by more than its color. */
  dashed?: boolean;
}

/** A key mirrors its mark: a short stroke for lines, a small rectangle for bars. */
export function Legend({ items, kind }: { items: LegendItem[]; kind: "line" | "bar" }) {
  return (
    <ul className="legend" aria-label="Legenda">
      {items.map((item) => (
        <li key={item.name}>
          <span className={`legend-key legend-key-${kind}`} style={{
              background: item.dashed
                ? `repeating-linear-gradient(90deg, ${item.color} 0 6px, transparent 6px 10px)`
                : item.color,
            }} aria-hidden />
          {item.name}
        </li>
      ))}
    </ul>
  );
}
