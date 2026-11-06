interface StatusBadgeProps {
  active: boolean;
  /** The register's noun is feminine (conta, categoria, forma): "Ativa", not "Ativo". */
  feminine?: boolean;
  /** Words for a register whose off state is not "inactive" (a recurrence is paused). */
  activeLabel?: string;
  inactiveLabel?: string;
}

/** Whether a record is in use. The dot is a shape and the word is text, so it never rests on color alone. */
export function StatusBadge({ active, feminine = false, activeLabel, inactiveLabel }: StatusBadgeProps) {
  const ending = feminine ? "a" : "o";
  return (
    <span className={active ? "badge badge-dot badge-ok" : "badge badge-dot badge-muted"}>
      {active ? (activeLabel ?? `Ativ${ending}`) : (inactiveLabel ?? `Inativ${ending}`)}
    </span>
  );
}
