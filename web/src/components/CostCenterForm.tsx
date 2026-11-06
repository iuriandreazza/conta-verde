import type { FormEvent } from "react";
import type { CostCenter, CostCenterInput } from "../api";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
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
  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) return;
    const form = new FormData(event.currentTarget);
    onSubmit({ name: String(form.get("name") ?? ""), active: form.get("active") === "on" });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="cost-center-info">
        <h3 id="cost-center-info">Dados do centro de custo</h3>
        <div className="field-grid">
          <label>
            Nome
            <input name="name" required maxLength={200} autoComplete="off" defaultValue={costCenter?.name} />
          </label>
          <label className="inline">
            <input name="active" type="checkbox" defaultChecked={costCenter?.active ?? true} /> Ativo
          </label>
        </div>
      </section>
      <Notice message={error} />
      <div className="form-footer">
        <Button type="button" variant="ghost" disabled={pending} onClick={onCancel}>
          Voltar
        </Button>
        <Button type="submit" icon={costCenter === null ? PlusIcon : CheckIcon} pending={pending}>
          {costCenter === null ? "Adicionar" : "Salvar alterações"}
        </Button>
      </div>
    </form>
  );
}
