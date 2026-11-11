import { useId, useState, type ReactNode } from "react";
import { useT } from "../../i18n";
import { InfoTip } from "../InfoTip";

interface ChartCardProps {
  title: string;
  description: string;
  legend?: ReactNode;
  /** The accessible twin of the chart: every value, readable without hover or color. */
  table: { columns: string[]; rows: string[][] };
  children: ReactNode;
}

export function ChartCard({ title, description, legend, table, children }: ChartCardProps) {
  const t = useT();
  const [showTable, setShowTable] = useState(false);
  const titleId = useId();
  return (
    <figure className="card chart-card" aria-labelledby={titleId}>
      <figcaption className="chart-head">
        <div>
          <div className="title-with-tip">
            <h2 id={titleId}>{title}</h2>
            <InfoTip label={t("About this chart: {title}", { title })} align="start">
              <p>{description}</p>
            </InfoTip>
          </div>
        </div>
        <button
          type="button"
          className="link-button"
          aria-pressed={showTable}
          onClick={() => setShowTable(!showTable)}
        >
          {showTable ? t("View chart") : t("View as table")}
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
