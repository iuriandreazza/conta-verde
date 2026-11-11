import { useRef, useState, type FormEvent } from "react";
import type { IssTaxation, NatureKind, OperationNature, TaxRegime, TaxRule } from "../api";
import { msg, useT } from "../i18n";
import { toApiDecimal, toInputDecimal } from "../lib/decimal";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { HelpTitle } from "./HelpTitle";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { useSheetDirty } from "./Sheet";

// The names of the tax regimes are the law's own words and are written the same in both languages.
export const REGIME_LABELS: Record<TaxRegime, string> = {
  simples_nacional: "Simples Nacional",
  lucro_presumido: "Lucro Presumido",
};

const MAX_RULES = 20;

/** A rule being typed: every field is text until the form is sent. */
interface RuleRow {
  key: number;
  regime: TaxRegime;
  ncm_prefix: string;
  icms_code: string;
  pis_code: string;
  cofins_code: string;
  icms_rate: string;
  pis_rate: string;
  cofins_rate: string;
}

const blankRule = (key: number, regime: TaxRegime): RuleRow => ({
  key,
  regime,
  ncm_prefix: "",
  icms_code: "",
  pis_code: "",
  cofins_code: "",
  icms_rate: "",
  pis_rate: "",
  cofins_rate: "",
});

const rowOf = (key: number, rule: TaxRule): RuleRow => ({
  key,
  regime: rule.regime,
  ncm_prefix: rule.ncm_prefix ?? "",
  icms_code: rule.icms_code,
  pis_code: rule.pis_code,
  cofins_code: rule.cofins_code,
  icms_rate: toInputDecimal(rule.icms_rate),
  pis_rate: toInputDecimal(rule.pis_rate),
  cofins_rate: toInputDecimal(rule.cofins_rate),
});

/** A rate as the API takes it: blank is not set, and a comma is written as a point. */
const rateOf = (text: string): string | null => toApiDecimal(text) ?? null;

const ISS_TAXATION_LABELS: Record<IssTaxation, string> = {
  taxable: msg("Taxable"),
  immune: msg("Immune"),
  export: msg("Export operation"),
  not_incident: msg("Not incident"),
};

/** The ISSQN terms of a service as they are typed. */
interface TermsFields {
  national_tax_code: string;
  municipal_service_code: string;
  iss_taxation: IssTaxation | "";
  iss_rate: string;
}

const termsOf = (nature: OperationNature | null): TermsFields => ({
  national_tax_code: nature?.service?.national_tax_code ?? "",
  municipal_service_code: nature?.service?.municipal_service_code ?? "",
  iss_taxation: nature?.service?.iss_taxation ?? "",
  iss_rate: toInputDecimal(nature?.service?.iss_rate ?? null),
});

interface NatureFormProps {
  /** The nature being changed, or `null` to add one. */
  nature: OperationNature | null;
  /** The company's regime: the one a new rule starts with. */
  regime: TaxRegime;
  onSubmit: (nature: OperationNature) => void;
  onCancel: () => void;
}

/**
 * Adds or changes an operation nature with its tax rules (ADR 0073). The product knows no tax table: the codes and
 * rates are the company's and its accountant's, and only their shape is checked, by the server when the settings are
 * saved. Nothing is sent from here: the nature goes back to the page, and the page's Salvar sends everything.
 */
export function NatureForm({ nature, regime, onSubmit, onCancel }: NatureFormProps) {
  const t = useT();
  const nextKey = useRef(0);
  const [kind, setKind] = useState<NatureKind>(nature?.kind ?? "product");
  const [rows, setRows] = useState<RuleRow[]>(() => (nature?.rules ?? []).map((rule) => rowOf(nextKey.current++, rule)));
  const [terms, setTerms] = useState<TermsFields>(() => termsOf(nature));
  const markDirty = useSheetDirty();
  const isProduct = kind === "product";
  // The ISSQN terms are optional until one is typed: then the national code and the taxation are both needed.
  const termsStarted = Object.values(terms).some((value) => value.trim() !== "");

  function change(key: number, update: Partial<RuleRow>) {
    setRows((current) => current.map((row) => (row.key === key ? { ...row, ...update } : row)));
  }

  function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const text = (name: string) => String(form.get(name) ?? "").trim();
    const optional = (name: string) => text(name) || null;
    onSubmit({
      code: text("code"),
      name: text("name"),
      kind,
      cfop_in_state: isProduct ? optional("cfop_in_state") : null,
      cfop_interstate: isProduct ? optional("cfop_interstate") : null,
      active: form.get("active") === "on",
      rules: isProduct
        ? rows.map((row) => ({
            regime: row.regime,
            ncm_prefix: row.ncm_prefix.trim() || null,
            icms_code: row.icms_code.trim(),
            pis_code: row.pis_code.trim(),
            cofins_code: row.cofins_code.trim(),
            icms_rate: rateOf(row.icms_rate),
            pis_rate: rateOf(row.pis_rate),
            cofins_rate: rateOf(row.cofins_rate),
          }))
        : [],
      service:
        !isProduct && termsStarted && terms.iss_taxation !== ""
          ? {
              national_tax_code: terms.national_tax_code.trim(),
              municipal_service_code: terms.municipal_service_code.trim() || null,
              iss_taxation: terms.iss_taxation,
              iss_rate: rateOf(terms.iss_rate),
            }
          : null,
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="nature-info">
        <HelpTitle id="nature-info" title={t("Nature details")}>
          <p>
            {t(
              "The name is what the person reads; the code is the stable key the fiscal document keeps and it does not change once saved. Goods take a CFOP and tax rules; a service takes the ISSQN, further down.",
            )}
          </p>
        </HelpTitle>
        <div className="field-grid">
          <label>
            {t("Name")}
            <input name="name" required maxLength={60} autoComplete="off" placeholder={t("Sale of goods")} defaultValue={nature?.name} />
          </label>
          <label>
            {t("Code")}
            <input
              name="code"
              required
              maxLength={30}
              pattern="[A-Za-z0-9_\-]+"
              autoComplete="off"
              placeholder={t("sale")}
              readOnly={nature !== null}
              defaultValue={nature?.code}
            />
          </label>
          <label>
            {t("Type")}
            <select
              name="kind"
              value={kind}
              onChange={(event) => {
                markDirty();
                setKind(event.target.value as NatureKind);
              }}
            >
              <option value="product">{t("Goods (NF-e / NFC-e)")}</option>
              <option value="service">{t("Service (NFS-e)")}</option>
            </select>
          </label>
          <label className="inline">
            <input name="active" type="checkbox" defaultChecked={nature?.active ?? true} /> {t("Active nature")}
          </label>
        </div>
      </section>

      {isProduct && (
        <>
          <section className="form-section" aria-labelledby="nature-cfop">
            <HelpTitle id="nature-cfop" title="CFOP">
              <p>
                {t(
                  "Four digits. The in-state CFOP starts with 5 and the one for another state with 6. The product has no CFOP table: confirm the right operation code with your accountant.",
                )}
              </p>
            </HelpTitle>
            <div className="field-grid">
              <label>
                {t("CFOP within the state")}
                <input name="cfop_in_state" required inputMode="numeric" maxLength={4} autoComplete="off" placeholder="5102" defaultValue={nature?.cfop_in_state ?? ""} />
              </label>
              <label>
                {t("Interstate CFOP")}
                <input name="cfop_interstate" required inputMode="numeric" maxLength={4} autoComplete="off" placeholder="6102" defaultValue={nature?.cfop_interstate ?? ""} />
              </label>
            </div>
          </section>

          <section className="form-section" aria-labelledby="nature-rules">
            <HelpTitle id="nature-rules" title={t("Tax rules")}>
              <p>
                {t(
                  "The codes and rates that go on the items of this nature, by regime and, if you want, by the start of the product's NCM (the rule with the longest prefix wins). ICMS: 3-digit CSOSN in the Simples Nacional, 2-digit CST in the Lucro Presumido; PIS and COFINS: 2-digit CST. Rates in %, with up to two decimals, and blank when they do not apply. Only the format is checked here: the values are the accountant's.",
                )}
              </p>
            </HelpTitle>
            {rows.length === 0 && <p className="muted">{t("No rules yet.")}</p>}
            {rows.map((row, index) => (
              <fieldset key={row.key} className="tax-rule">
                <legend>{t("Rule {number}", { number: index + 1 })}</legend>
                <div className="field-grid">
                  <label>
                    {t("Regime")}
                    <select
                      aria-label={t("Regime of rule {number}", { number: index + 1 })}
                      value={row.regime}
                      onChange={(event) => change(row.key, { regime: event.target.value as TaxRegime })}
                    >
                      {(Object.keys(REGIME_LABELS) as TaxRegime[]).map((key) => (
                        <option key={key} value={key}>
                          {REGIME_LABELS[key]}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    {t("NCM prefix")}
                    <input
                      aria-label={t("NCM prefix of rule {number}", { number: index + 1 })}
                      inputMode="numeric"
                      maxLength={8}
                      autoComplete="off"
                      placeholder={t("All products")}
                      value={row.ncm_prefix}
                      onChange={(event) => change(row.key, { ncm_prefix: event.target.value })}
                    />
                  </label>
                  <label>
                    {row.regime === "simples_nacional" ? t("ICMS CSOSN") : t("ICMS CST")}
                    <input
                      aria-label={t("ICMS of rule {number}", { number: index + 1 })}
                      inputMode="numeric"
                      maxLength={3}
                      autoComplete="off"
                      value={row.icms_code}
                      onChange={(event) => change(row.key, { icms_code: event.target.value })}
                    />
                  </label>
                  <label>
                    {t("ICMS rate (%)")}
                    <input
                      aria-label={t("ICMS rate of rule {number}", { number: index + 1 })}
                      inputMode="decimal"
                      autoComplete="off"
                      value={row.icms_rate}
                      onChange={(event) => change(row.key, { icms_rate: event.target.value })}
                    />
                  </label>
                  <label>
                    {t("PIS CST")}
                    <input
                      aria-label={t("PIS of rule {number}", { number: index + 1 })}
                      inputMode="numeric"
                      maxLength={2}
                      autoComplete="off"
                      value={row.pis_code}
                      onChange={(event) => change(row.key, { pis_code: event.target.value })}
                    />
                  </label>
                  <label>
                    {t("PIS rate (%)")}
                    <input
                      aria-label={t("PIS rate of rule {number}", { number: index + 1 })}
                      inputMode="decimal"
                      autoComplete="off"
                      value={row.pis_rate}
                      onChange={(event) => change(row.key, { pis_rate: event.target.value })}
                    />
                  </label>
                  <label>
                    {t("COFINS CST")}
                    <input
                      aria-label={t("COFINS of rule {number}", { number: index + 1 })}
                      inputMode="numeric"
                      maxLength={2}
                      autoComplete="off"
                      value={row.cofins_code}
                      onChange={(event) => change(row.key, { cofins_code: event.target.value })}
                    />
                  </label>
                  <label>
                    {t("COFINS rate (%)")}
                    <input
                      aria-label={t("COFINS rate of rule {number}", { number: index + 1 })}
                      inputMode="decimal"
                      autoComplete="off"
                      value={row.cofins_rate}
                      onChange={(event) => change(row.key, { cofins_rate: event.target.value })}
                    />
                  </label>
                </div>
                <div className="form-actions">
                  <Button
                    type="button"
                    variant="ghost"
                    className="btn-small"
                    aria-label={t("Remove rule {number}", { number: index + 1 })}
                    onClick={() => {
                      markDirty();
                      setRows((current) => current.filter((item) => item.key !== row.key));
                    }}
                  >
                    {t("Remove")}
                  </Button>
                </div>
              </fieldset>
            ))}
            <div className="form-actions">
              <Button
                type="button"
                variant="ghost"
                className="btn-small"
                icon={PlusIcon}
                disabled={rows.length >= MAX_RULES}
                onClick={() => {
                  markDirty();
                  setRows((current) => [...current, blankRule(nextKey.current++, regime)]);
                }}
              >
                {t("Add rule")}
              </Button>
            </div>
          </section>
        </>
      )}

      {!isProduct && (
        <section className="form-section" aria-labelledby="nature-iss">
          <HelpTitle id="nature-iss" title={t("Service ISSQN")}>
            <p>
              {t(
                "What the NFS-e says about the tax on this service: the national code (six digits: item, subitem and breakdown of the national list of services), the municipality's own code when the gateway asks for it, how the ISS is charged and, if the company defines it, the rate in %, with up to two decimals. Without the national code and the way it is charged no NFS-e is issued with this nature. The product has no table of services and does not choose the taxation: confirm with your accountant.",
              )}
            </p>
          </HelpTitle>
          <div className="field-grid">
            <label>
              {t("National service code")}
              <input
                inputMode="numeric"
                maxLength={6}
                pattern="[0-9]{6}"
                autoComplete="off"
                placeholder="010701"
                required={termsStarted}
                value={terms.national_tax_code}
                onChange={(event) => {
                  markDirty();
                  setTerms({ ...terms, national_tax_code: event.target.value });
                }}
              />
            </label>
            <label>
              {t("Municipal service code")}
              <input
                maxLength={20}
                autoComplete="off"
                value={terms.municipal_service_code}
                onChange={(event) => {
                  markDirty();
                  setTerms({ ...terms, municipal_service_code: event.target.value });
                }}
              />
            </label>
            <label>
              {t("ISS taxation")}
              <select
                required={termsStarted}
                value={terms.iss_taxation}
                onChange={(event) => {
                  markDirty();
                  setTerms({ ...terms, iss_taxation: event.target.value as IssTaxation | "" });
                }}
              >
                <option value="">{t("Not defined yet")}</option>
                {(Object.keys(ISS_TAXATION_LABELS) as IssTaxation[]).map((key) => (
                  <option key={key} value={key}>
                    {t(ISS_TAXATION_LABELS[key])}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("ISS rate (%)")}
              <input
                inputMode="decimal"
                autoComplete="off"
                value={terms.iss_rate}
                onChange={(event) => {
                  markDirty();
                  setTerms({ ...terms, iss_rate: event.target.value });
                }}
              />
            </label>
          </div>
        </section>
      )}

      <div className="form-footer">
        <BackButton onClick={onCancel} />
        <Button type="submit" icon={nature === null ? PlusIcon : CheckIcon}>
          {nature === null ? t("Add nature") : t("Finish nature")}
        </Button>
      </div>
    </form>
  );
}
