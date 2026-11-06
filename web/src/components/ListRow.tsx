import { motion } from "motion/react";
import type { ReactNode } from "react";

// Rows enter one after another, but never wait longer than the eighth for a long page.
const MAX_STAGGERED_ROWS = 8;

interface ListRowProps {
  /** Position in the list, which sets how long the row waits to enter. */
  index: number;
  /** A freshly added row flashes green and fades back to transparent. */
  highlighted: boolean;
  /** An edge mark for a row that needs attention; the row says why in words too (a status badge). */
  tone?: "critical";
  children: ReactNode;
}

export function ListRow({ index, highlighted, tone, children }: ListRowProps) {
  const flash = (alpha: 0 | 1) => `rgba(47, 178, 110, ${highlighted ? alpha * 0.25 : 0})`;
  return (
    <motion.tr
      className={tone && `row-${tone}`}
      initial={{ opacity: 0, y: 8, backgroundColor: flash(1) }}
      animate={{ opacity: 1, y: 0, backgroundColor: flash(0) }}
      transition={{
        delay: Math.min(index, MAX_STAGGERED_ROWS) * 0.03,
        duration: highlighted ? 1.4 : 0.2,
      }}
    >
      {children}
    </motion.tr>
  );
}
