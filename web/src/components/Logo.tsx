import { motion } from "motion/react";
import { useId } from "react";

interface LogoProps {
  size?: number;
  /** Green tile behind the leaf. Off where the mark sits on a green surface and the tile would disappear. */
  tile?: boolean;
  /** Draws the leaf and the rising line once when the logo first appears. */
  animated?: boolean;
}

// Keep the geometry in sync with public/favicon.svg.
export function LogoMark({ size = 32, tile = true, animated = false }: LogoProps) {
  const gradientId = useId();
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden focusable="false">
      {tile && (
        <>
          <defs>
            <linearGradient id={gradientId} x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="#2fb26e" />
              <stop offset="1" stopColor="#14583a" />
            </linearGradient>
          </defs>
          <rect width="64" height="64" rx="16" fill={`url(#${gradientId})`} />
        </>
      )}
      <motion.path
        d="M13 51V32a19 19 0 0 1 19-19h19v19a19 19 0 0 1-19 19Z"
        fill="#fff"
        style={{ transformOrigin: "32px 32px" }}
        initial={animated ? { scale: 0.6, opacity: 0 } : false}
        animate={{ scale: 1, opacity: 1 }}
        transition={{ type: "spring", stiffness: 260, damping: 20 }}
      />
      <motion.path
        d="M22 42l8-8 5 5 9-12"
        fill="none"
        stroke="#17603c"
        strokeWidth={4.5}
        strokeLinecap="round"
        strokeLinejoin="round"
        initial={animated ? { pathLength: 0 } : false}
        animate={{ pathLength: 1 }}
        transition={{ delay: 0.25, duration: 0.6, ease: "easeOut" }}
      />
    </svg>
  );
}

export function Logo({ size = 32, tile = true, animated = false }: LogoProps) {
  return (
    <span className="logo">
      <LogoMark size={size} tile={tile} animated={animated} />
      <span className="wordmark">
        conta<span className="wordmark-accent">-verde</span>
      </span>
    </span>
  );
}
