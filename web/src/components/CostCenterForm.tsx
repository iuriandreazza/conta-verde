import type { FormEvent } from "react";
import type { CostCenter, CostCenterInput } from "../api";
import { Button } from "./Button";
import { BackButton } from "./BackButton";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { useT } from "../i18n";
import { Notice } from "./Notice";

interface CostCenterFormProps {
  /** The cost center being changed, or `null` to register one. */
  costCenter: CostCenter | null;
  pending: boolean;
  error: string | null;
  onSubmit: (input: CostCenterInput) => void;
  onCancel: () => void;
}

/** Registers a cost center, or changes one. Remount it (a `key`) to start over. */
export function CostCenterForm({ costCenter, pending, error, onSubmit, onCancel }: CostCenterFormProps) {
  const t = useT();
  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    onSubmit({ name: String(form.get("name") ?? ""), active: form.get("active") === "on" });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="cost-center-info">
        <h3 id="cost-center-info">{t("Cost center details")}</h3>
        <div className="field-grid">
          <label>
            {t("Name")}
            <input name="name" required maxLength={200} autoComplete="off" defaultValue={costCenter?.name} />
          </label>
          <label className="inline">
            <input name="active" type="checkbox" defaultChecked={costCenter?.active ?? true} /> {t("Active")}
          </label>
        </div>
      </section>
      <Notice message={error} />
      <div className="form-footer">
        <BackButton disabled={pending} onClick={onCancel} />
        <Button type="submit" icon={costCenter === null ? PlusIcon : CheckIcon} pending={pending}>
          {costCenter === null ? t("Add") : t("Save changes")}
        </Button>
      </div>
    </form>
  );
}
