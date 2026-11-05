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
  children: ReactNode;
}

export function Button({
  icon: Icon,
  pending = false,
  variant = "primary",
  className,
  disabled,
  children,
  ...props
}: ButtonProps) {
  const iconRef = useRef<IconHandle>(null);
  const inactive = disabled || pending;

  // The whole button is the hover target, not just the 16px icon inside it.
  const play = () => iconRef.current?.startAnimation();
  const rest = () => iconRef.current?.stopAnimation();

  return (
    <motion.button
      className={cn("btn", `btn-${variant}`, className)}
      disabled={inactive}
      whileHover={inactive ? undefined : { y: -1 }}
      whileTap={inactive ? undefined : { scale: 0.97 }}
      transition={{ type: "spring", stiffness: 500, damping: 30 }}
      onMouseEnter={play}
      onMouseLeave={rest}
      onFocus={play}
      onBlur={rest}
      {...props}
    >
      {pending ? <Spinner size={16} /> : Icon && <Icon ref={iconRef} size={16} aria-hidden />}
      <span>{children}</span>
    </motion.button>
  );
}
