import { useT } from "../i18n";

interface StatusBadgeProps {
  active: boolean;
  /** The register's noun is feminine in Portuguese (conta, categoria, forma): "Ativa", not "Ativo". */
  feminine?: boolean;
  /** Words for a register whose off state is not "inactive" (a recurrence is paused), already in the person's language. */
  activeLabel?: string;
  inactiveLabel?: string;
}

/** Whether a record is in use. The dot is a shape and the word is text, so it never rests on color alone. */
export function StatusBadge({ active, feminine = false, activeLabel, inactiveLabel }: StatusBadgeProps) {
  const t = useT();
  // English has one word for both genders, so the feminine keys carry an empty `{feminine}` part that only Portuguese tells apart.
  const words = feminine
    ? { on: t("Active{feminine}", { feminine: "" }), off: t("Inactive{feminine}", { feminine: "" }) }
    : { on: t("Active"), off: t("Inactive") };
  return (
    <span className={active ? "badge badge-dot badge-ok" : "badge badge-dot badge-muted"}>
      {active ? (activeLabel ?? words.on) : (inactiveLabel ?? words.off)}
    </span>
  );
}
