import { motion } from "motion/react";

/** Decorative: pair it with visible or screen-reader text that says what is loading. */
export function Spinner({ size = 20 }: { size?: number }) {
  return (
    <motion.svg
      aria-hidden
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={3}
      strokeLinecap="round"
      animate={{ rotate: 360 }}
      transition={{ repeat: Infinity, duration: 0.8, ease: "linear" }}
    >
      <circle cx="12" cy="12" r="9" strokeOpacity={0.25} />
      <path d="M21 12a9 9 0 0 0-9-9" />
    </motion.svg>
  );
}
