import { useRef, useState, type FormEvent } from "react";
import {
  errorMessage,
  type AddressInput,
  type Contact,
  type Party,
  type PartyInput,
  type PostalCodeEntry,
} from "../api";
import { Button } from "./Button";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { Notice, type NoticeTone } from "./Notice";

const STATES = [
  "AC", "AL", "AM", "AP", "BA", "CE", "DF", "ES", "GO", "MA", "MG", "MS", "MT", "PA",
  "PB", "PE", "PI", "PR", "RJ", "RN", "RO", "RR", "RS", "SC", "SE", "SP", "TO",
];

/** The API keeps the CEP as eight digits; people write it as 01310-100. */
const withHyphen = (postalCode: string) => postalCode.replace(/^(\d{5})(\d{3})$/, "$1-$2");
const digitsOf = (text: string) => text.replace(/\D/g, "");

const MAX_CONTACTS = 20;

/** A contact being typed: every field is text until the form is sent. */
interface ContactRow {
  key: number;
  name: string;
  job_title: string;
  email: string;
  phone: string;
}

const rowOf = (key: number, contact?: Contact): ContactRow => ({
  key,
  name: contact?.name ?? "",
  job_title: contact?.job_title ?? "",
  email: contact?.email ?? "",
  phone: contact?.phone ?? "",
});

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
  /** Finds the address of a postal code; it asks a third party, so it may fail. */
  lookupAddress: (postalCode: string) => Promise<PostalCodeEntry>;
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
  lookupAddress,
}: PartyFormProps) {
  const formRef = useRef<HTMLFormElement>(null);
  // Controlled, unlike the other fields, because the lookup button waits for eight digits.
  const [postalCode, setPostalCode] = useState(
    party?.address ? withHyphen(party.address.postal_code) : "",
  );
  // The CEP as it is now, for a lookup that finishes after the person has changed it.
  const currentPostalCode = useRef(postalCode);
  currentPostalCode.current = postalCode;
  const [lookingUp, setLookingUp] = useState(false);
  const [lookupNotice, setLookupNotice] = useState<{ message: string; tone: NoticeTone } | null>(
    null,
  );
  const nextKey = useRef(0);
  const [rows, setRows] = useState<ContactRow[]>(() =>
    (party?.contacts ?? []).map((contact) => rowOf(nextKey.current++, contact)),
  );

  // The row just added takes the cursor, so a keyboard user can type the name straight away.
  const [addedKey, setAddedKey] = useState<number | null>(null);

  function addRow() {
    const key = nextKey.current++;
    setAddedKey(key);
    setRows((current) => [...current, rowOf(key)]);
  }

  function update(key: number, change: Partial<ContactRow>) {
    setRows((current) => current.map((row) => (row.key === key ? { ...row, ...change } : row)));
  }

  // The user asked for the address, so what the service knows replaces what is typed; the number
  // and the complement are never touched.
  async function lookUp() {
    setLookingUp(true);
    setLookupNotice(null);
    try {
      const asked = postalCode;
      const entry = await lookupAddress(asked);
      // An answer about another CEP than the one shown would fill in an address that is not its.
      if (digitsOf(currentPostalCode.current) !== digitsOf(asked)) {
        return;
      }
      const elements = formRef.current?.elements;
      const fill = (name: string, value: string | null) => {
        const field = elements?.namedItem(name);
        if (value && (field instanceof HTMLInputElement || field instanceof HTMLSelectElement)) {
          field.value = value;
        }
      };
      fill("street", entry.street);
      fill("district", entry.district);
      fill("city", entry.city);
      fill("state", entry.state);
      // The code that was asked, tidied: a stray letter in the field would be refused on saving.
      setPostalCode(withHyphen(digitsOf(asked)));
      const next = elements?.namedItem(entry.street ? "number" : "street");
      if (next instanceof HTMLInputElement) {
        next.focus();
      }
      setLookupNotice({
        tone: "success",
        message: entry.street
          ? "Endereço preenchido. Confira e informe o número."
          : "Cidade e UF preenchidas: este CEP não tem rua. Informe o logradouro.",
      });
    } catch (failure) {
      setLookupNotice({ tone: "warning", message: errorMessage(failure) });
    } finally {
      setLookingUp(false);
    }
  }

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
    // A contact row left empty is no contact; one with only a phone goes as it is and the API
    // says that the name is missing.
    const contacts = rows
      .filter((row) => [row.name, row.job_title, row.email, row.phone].some((value) => value.trim()))
      .map((row) => ({
        name: row.name.trim(),
        job_title: row.job_title.trim() || undefined,
        email: row.email.trim() || undefined,
        phone: row.phone.trim() || undefined,
      }));
    const tags = text("tags")
      .split(",")
      .map((tag) => tag.trim())
      .filter(Boolean);
    onSubmit({
      name: String(form.get("name") ?? ""),
      document: optional("document"),
      email: optional("email"),
      is_customer: form.get("is_customer") === "on",
      is_supplier: form.get("is_supplier") === "on",
      is_carrier: form.get("is_carrier") === "on",
      is_seller: form.get("is_seller") === "on",
      address,
      contacts,
      tags,
    });
  }

  const address = party?.address;
  return (
    <form ref={formRef} onSubmit={handleSubmit}>
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
      <fieldset className="address-fields">
        <legend>Endereço</legend>
        <label className="span-4">
          CEP
          <input
            name="postal_code"
            inputMode="numeric"
            autoComplete="off"
            placeholder="00000-000"
            maxLength={9}
            value={postalCode}
            onChange={(event) => {
              setPostalCode(event.target.value);
              // What the last lookup said was about the old code.
              setLookupNotice(null);
            }}
          />
        </label>
        <div className="lookup-action">
          <Button
            type="button"
            variant="ghost"
            className="btn-small"
            pending={lookingUp}
            disabled={digitsOf(postalCode).length !== 8}
            onClick={() => void lookUp()}
          >
            Buscar endereço
          </Button>
        </div>
        {lookupNotice && (
          <div className="address-notice">
            <Notice message={lookupNotice.message} tone={lookupNotice.tone} />
          </div>
        )}
        <label className="span-4">
          Logradouro
          <input name="street" maxLength={150} autoComplete="off" defaultValue={address?.street ?? ""} />
        </label>
        <label className="span-2">
          Número
          <input name="number" maxLength={20} autoComplete="off" defaultValue={address?.number ?? ""} />
        </label>
        <label className="span-3">
          Complemento
          <input
            name="complement"
            maxLength={80}
            autoComplete="off"
            defaultValue={address?.complement ?? ""}
          />
        </label>
        <label className="span-3">
          Bairro
          <input name="district" maxLength={80} autoComplete="off" defaultValue={address?.district ?? ""} />
        </label>
        <label className="span-4">
          Cidade
          <input name="city" maxLength={80} autoComplete="off" defaultValue={address?.city ?? ""} />
        </label>
        <label className="span-2">
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
      <label>
        Etiquetas
        <input
          name="tags"
          autoComplete="off"
          placeholder="Separadas por vírgula, como vip, atacado"
          defaultValue={party?.tags.join(", ") ?? ""}
        />
      </label>
      <fieldset className="contacts">
        <legend>Contatos</legend>
        {rows.map((row, index) => (
          <div className="contact-row" key={row.key}>
            <label className="span-2">
              Nome
              <input
                autoComplete="off"
                maxLength={100}
                autoFocus={row.key === addedKey}
                aria-label={`Nome do contato ${index + 1}`}
                value={row.name}
                onChange={(event) => update(row.key, { name: event.target.value })}
              />
            </label>
            <label>
              Cargo
              <input
                autoComplete="off"
                maxLength={80}
                aria-label={`Cargo do contato ${index + 1}`}
                value={row.job_title}
                onChange={(event) => update(row.key, { job_title: event.target.value })}
              />
            </label>
            <label>
              Telefone
              <input
                type="tel"
                autoComplete="off"
                maxLength={30}
                aria-label={`Telefone do contato ${index + 1}`}
                value={row.phone}
                onChange={(event) => update(row.key, { phone: event.target.value })}
              />
            </label>
            <label className="span-2">
              E-mail
              <input
                type="email"
                autoComplete="off"
                aria-label={`E-mail do contato ${index + 1}`}
                value={row.email}
                onChange={(event) => update(row.key, { email: event.target.value })}
              />
            </label>
            <Button
              type="button"
              variant="ghost"
              className="btn-small"
              aria-label={`Remover contato ${index + 1}`}
              onClick={() => setRows((current) => current.filter((item) => item.key !== row.key))}
            >
              Remover
            </Button>
          </div>
        ))}
        <div className="form-actions">
          <Button
            type="button"
            variant="ghost"
            className="btn-small"
            icon={PlusIcon}
            disabled={rows.length >= MAX_CONTACTS}
            onClick={addRow}
          >
            Adicionar contato
          </Button>
        </div>
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
