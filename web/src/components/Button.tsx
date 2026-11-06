import { motion } from "motion/react";
import {
  useRef,
  type ButtonHTMLAttributes,
  type ForwardRefExoticComponent,
  type HTMLAttributes,
  type ReactNode,
  type RefAttributes,
} from "react";
import { cn } from "../lib/utils";
import { Spinner } from "./Spinner";

interface IconHandle {
  startAnimation: () => void;
  stopAnimation: () => void;
}

/** Any icon from components/icons: it animates on its own hover, or on command through its handle. */
export type AnimatedIcon = ForwardRefExoticComponent<
  HTMLAttributes<HTMLDivElement> & { size?: number } & RefAttributes<IconHandle>
>;

interface ButtonProps
  extends Omit<
    ButtonHTMLAttributes<HTMLButtonElement>,
    "onAnimationStart" | "onDrag" | "onDragStart" | "onDragEnd"
  > {
  icon?: AnimatedIcon;
  pending?: boolean;
  variant?: "primary" | "ghost";
  /**
   * Shows the icon alone, for the actions that repeat on every row. The children stay as the name a
   * screen reader reads and the tooltip a pointer sees, so they still say what the button does.
   */
  iconOnly?: boolean;
  children: ReactNode;
}

export function Button({
  icon: Icon,
  pending = false,
  variant = "primary",
  iconOnly = false,
  className,
  disabled,
  children,
  onMouseEnter,
  onMouseLeave,
  onFocus,
  onBlur,
  title,
  ...props
}: ButtonProps) {
  const iconRef = useRef<IconHandle>(null);
  const inactive = disabled || pending;
  // Without an icon there would be nothing to show, so the words stay.
  const iconAlone = iconOnly && Icon !== undefined;

  // The whole button is the hover target, not just the 16px icon inside it. Caller handlers still
  // run: they are called after the icon reacts instead of replacing it.
  const play = () => iconRef.current?.startAnimation();
  const rest = () => iconRef.current?.stopAnimation();

  return (
    <motion.button
      className={cn("btn", `btn-${variant}`, iconAlone && "btn-icon", className)}
      title={title ?? (iconAlone ? props["aria-label"] : undefined)}
      disabled={inactive}
      aria-busy={pending || undefined}
      whileHover={inactive ? undefined : { y: -1 }}
      whileTap={inactive ? undefined : { scale: 0.97 }}
      transition={{ type: "spring", stiffness: 500, damping: 30 }}
      onMouseEnter={(event) => {
        play();
        onMouseEnter?.(event);
      }}
      onMouseLeave={(event) => {
        rest();
        onMouseLeave?.(event);
      }}
      onFocus={(event) => {
        play();
        onFocus?.(event);
      }}
      onBlur={(event) => {
        rest();
        onBlur?.(event);
      }}
      {...props}
    >
      {pending ? <Spinner size={16} /> : Icon && <Icon ref={iconRef} size={16} aria-hidden />}
      <span className={iconAlone ? "visually-hidden" : undefined}>{children}</span>
    </motion.button>
  );
}
