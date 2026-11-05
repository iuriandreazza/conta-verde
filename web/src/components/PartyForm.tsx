import type { FormEvent } from "react";
import type { AddressInput, Party, PartyInput } from "../api";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice, type NoticeTone } from "./Notice";

const STATES = [
  "AC", "AL", "AM", "AP", "BA", "CE", "DF", "ES", "GO", "MA", "MG", "MS", "MT", "PA",
  "PB", "PE", "PI", "PR", "RJ", "RN", "RO", "RR", "RS", "SC", "SE", "SP", "TO",
];

interface PartyFormProps {
  /** The party being changed, or `null` to register one. */
  party: Party | null;
  focusName: boolean;
  pending: boolean;
  /** Something else on the page is writing: wait for it. */
  blocked: boolean;
  error: string | null;
  notice: { message: string; tone: NoticeTone } | null;
  onSubmit: (input: PartyInput) => void;
  onCancel: () => void;
}

/**
 * Registers a party, or changes one: the form is the whole party either way, since a change
 * replaces every field. Remount it (a `key`) to start over.
 */
export function PartyForm({
  party,
  focusName,
  pending,
  blocked,
  error,
  notice,
  onSubmit,
  onCancel,
}: PartyFormProps) {
  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (pending) {
      return;
    }
    const form = new FormData(event.currentTarget);
    const text = (name: string) => String(form.get(name) ?? "").trim();
    const optional = (name: string) => text(name) || undefined;
    const fields = {
      postal_code: text("postal_code"),
      street: text("street"),
      number: optional("number"),
      complement: optional("complement"),
      district: optional("district"),
      city: text("city"),
      state: text("state"),
    };
    // An address left blank is no address; a partly filled one goes as it is and the API says
    // which part is missing.
    const hasAddress = Object.values(fields).some((value) => value !== undefined && value !== "");
    const address: AddressInput | undefined = hasAddress ? fields : undefined;
    onSubmit({
      name: String(form.get("name") ?? ""),
      document: optional("document"),
      email: optional("email"),
      is_customer: form.get("is_customer") === "on",
      is_supplier: form.get("is_supplier") === "on",
      is_carrier: form.get("is_carrier") === "on",
      is_seller: form.get("is_seller") === "on",
      address,
    });
  }

  const address = party?.address;
  return (
    <form onSubmit={handleSubmit}>
      <label>
        Nome
        <input
          name="name"
          required
          maxLength={200}
          autoFocus={focusName}
          defaultValue={party?.name}
        />
      </label>
      <label>
        CPF ou CNPJ
        {/* No numeric keypad: a CNPJ may contain letters since July 2026. */}
        <input
          name="document"
          autoCapitalize="characters"
          autoComplete="off"
          spellCheck={false}
          defaultValue={party?.document ?? ""}
        />
      </label>
      <label>
        E-mail
        <input name="email" type="email" defaultValue={party?.email ?? ""} />
      </label>
      <fieldset>
        <legend>Tipo</legend>
        <label className="inline">
          <input name="is_customer" type="checkbox" defaultChecked={party?.is_customer ?? true} />{" "}
          Cliente
        </label>
        <label className="inline">
          <input name="is_supplier" type="checkbox" defaultChecked={party?.is_supplier ?? false} />{" "}
          Fornecedor
        </label>
        <label className="inline">
          <input name="is_carrier" type="checkbox" defaultChecked={party?.is_carrier ?? false} />{" "}
          Transportadora
        </label>
        <label className="inline">
          <input name="is_seller" type="checkbox" defaultChecked={party?.is_seller ?? false} />{" "}
          Vendedor
        </label>
      </fieldset>
      <fieldset>
        <legend>Endereço</legend>
        <label>
          CEP
          <input
            name="postal_code"
            inputMode="numeric"
            autoComplete="off"
            placeholder="00000-000"
            maxLength={9}
            defaultValue={address?.postal_code ?? ""}
          />
        </label>
        <label>
          Logradouro
          <input name="street" maxLength={150} autoComplete="off" defaultValue={address?.street ?? ""} />
        </label>
        <label>
          Número
          <input name="number" maxLength={20} autoComplete="off" defaultValue={address?.number ?? ""} />
        </label>
        <label>
          Complemento
          <input
            name="complement"
            maxLength={80}
            autoComplete="off"
            defaultValue={address?.complement ?? ""}
          />
        </label>
        <label>
          Bairro
          <input name="district" maxLength={80} autoComplete="off" defaultValue={address?.district ?? ""} />
        </label>
        <label>
          Cidade
          <input name="city" maxLength={80} autoComplete="off" defaultValue={address?.city ?? ""} />
        </label>
        <label>
          UF
          <select name="state" defaultValue={address?.state ?? ""}>
            <option value=""></option>
            {STATES.map((state) => (
              <option key={state} value={state}>
                {state}
              </option>
            ))}
          </select>
        </label>
      </fieldset>
      <Notice message={error} />
      <Notice message={notice?.message ?? null} tone={notice?.tone} />
      <div className="form-actions">
        <Button
          type="submit"
          icon={party === null ? PlusIcon : CheckIcon}
          pending={pending}
          disabled={blocked}
        >
          {party === null ? "Adicionar" : "Salvar alterações"}
        </Button>
        {party !== null && (
          <Button type="button" variant="ghost" disabled={pending} onClick={onCancel}>
            Cancelar
          </Button>
        )}
      </div>
    </form>
  );
}
