export interface TileOption<K extends string> {
  key: K;
  label: string;
  /** `null` while the count is not known: the tile still filters. */
  count: number | null;
}

/**
 * A row of tiles that each filter a list to one value and say how many rows that is. The chosen one is
 * pressed; choosing it again does nothing, since "all" is a tile of its own.
 */
export function FilterTiles<K extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: K;
  options: TileOption<K>[];
  onChange: (key: K) => void;
}) {
  return (
    <div className="filter-tiles" role="group" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.key}
          type="button"
          className="filter-tile"
          aria-pressed={option.key === value}
          onClick={() => onChange(option.key)}
        >
          <span className="filter-tile-label">{option.label}</span>
          <strong className="num">{option.count ?? "–"}</strong>
        </button>
      ))}
    </div>
  );
}
