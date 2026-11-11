import { useT } from "../i18n";
import { Button } from "./Button";
import { useSheetGuard } from "./Sheet";

/** "Back" in a sheet's form: it asks before throwing away what was typed. */
export function BackButton({ disabled, onClick }: { disabled?: boolean; onClick: () => void }) {
  const t = useT();
  const guard = useSheetGuard();
  return (
    <Button type="button" variant="ghost" disabled={disabled} onClick={() => guard(onClick)}>
      {t("Back")}
    </Button>
  );
}
