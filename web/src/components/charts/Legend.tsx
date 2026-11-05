export interface LegendItem {
  name: string;
  color: string;
}

/** A key mirrors its mark: a short stroke for lines, a small rectangle for bars. */
export function Legend({ items, kind }: { items: LegendItem[]; kind: "line" | "bar" }) {
  return (
    <ul className="legend" aria-label="Legenda">
      {items.map((item) => (
        <li key={item.name}>
          <span className={`legend-key legend-key-${kind}`} style={{ background: item.color }} aria-hidden />
          {item.name}
        </li>
      ))}
    </ul>
  );
}
