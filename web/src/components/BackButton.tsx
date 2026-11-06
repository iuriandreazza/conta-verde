import { Button } from "./Button";
import { useSheetGuard } from "./Sheet";

/** "Voltar" in a sheet's form: it asks before throwing away what was typed. */
export function BackButton({ disabled, onClick }: { disabled?: boolean; onClick: () => void }) {
  const guard = useSheetGuard();
  return (
    <Button type="button" variant="ghost" disabled={disabled} onClick={() => guard(onClick)}>
      Voltar
    </Button>
  );
}
