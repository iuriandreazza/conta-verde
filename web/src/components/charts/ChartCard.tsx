import { useId, useState, type ReactNode } from "react";

interface ChartCardProps {
  title: string;
  description: string;
  legend?: ReactNode;
  /** The accessible twin of the chart: every value, readable without hover or color. */
  table: { columns: string[]; rows: string[][] };
  children: ReactNode;
}

export function ChartCard({ title, description, legend, table, children }: ChartCardProps) {
  const [showTable, setShowTable] = useState(false);
  const titleId = useId();
  return (
    <figure className="card chart-card" aria-labelledby={titleId}>
      <figcaption className="chart-head">
        <div>
          <h2 id={titleId}>{title}</h2>
          <p className="muted">{description}</p>
        </div>
        <button
          type="button"
          className="link-button"
          aria-pressed={showTable}
          onClick={() => setShowTable(!showTable)}
        >
          {showTable ? "Ver gráfico" : "Ver como tabela"}
        </button>
      </figcaption>
      {legend}
      {showTable ? (
        <div className="table-wrap">
          <table className="data-table">
            <caption className="visually-hidden">{title}</caption>
            <thead>
              <tr>
                {table.columns.map((column, index) => (
                  <th key={column} className={index > 0 ? "num" : undefined}>
                    {column}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {table.rows.map((row) => (
                <tr key={row[0]}>
                  {row.map((cell, index) => (
                    <td key={index} className={index > 0 ? "num" : undefined}>
                      {cell}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        children
      )}
    </figure>
  );
}
