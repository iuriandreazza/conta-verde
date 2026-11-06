import { useRef } from "react";

/** The latest value that was not `null`: a sheet keeps showing what it was about while it slides away. */
export function useLastValue<T>(value: T | null): T | null {
  const last = useRef<T | null>(null);
  if (value !== null) last.current = value;
  return value ?? last.current;
}
