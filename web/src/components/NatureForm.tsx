import { useRef, useState, type FormEvent } from "react";
import type { NatureKind, OperationNature, TaxRegime, TaxRule } from "../api";
import { toApiDecimal, toInputDecimal } from "../lib/decimal";
import { BackButton } from "./BackButton";
import { Button } from "./Button";
import { HelpTitle } from "./HelpTitle";
import { CheckIcon } from "./icons/check";
import { PlusIcon } from "./icons/plus";
import { useSheetDirty } from "./Sheet";

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
  const nextKey = useRef(0);
  const [kind, setKind] = useState<NatureKind>(nature?.kind ?? "product");
  const [rows, setRows] = useState<RuleRow[]>(() => (nature?.rules ?? []).map((rule) => rowOf(nextKey.current++, rule)));
  const markDirty = useSheetDirty();
  const isProduct = kind === "product";

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
    });
  }

  return (
    <form className="record-form" onSubmit={handleSubmit}>
      <section className="form-section" aria-labelledby="nature-info">
        <HelpTitle id="nature-info" title="Dados da natureza">
          <p>
            O nome é o que a pessoa lê; o código é a chave estável que o documento fiscal guarda e não muda depois de
            salvo. Mercadoria leva CFOP; serviço não leva CFOP nem regras aqui (o imposto de um serviço é decidido na
            NFS-e).
          </p>
        </HelpTitle>
        <div className="field-grid">
          <label>
            Nome
            <input name="name" required maxLength={60} autoComplete="off" placeholder="Venda de mercadoria" defaultValue={nature?.name} />
          </label>
          <label>
            Código
            <input
              name="code"
              required
              maxLength={30}
              pattern="[A-Za-z0-9_\-]+"
              autoComplete="off"
              placeholder="venda"
              readOnly={nature !== null}
              defaultValue={nature?.code}
            />
          </label>
          <label>
            Tipo
            <select
              name="kind"
              value={kind}
              onChange={(event) => {
                markDirty();
                setKind(event.target.value as NatureKind);
              }}
            >
              <option value="product">Mercadoria (NF-e / NFC-e)</option>
              <option value="service">Serviço (NFS-e)</option>
            </select>
          </label>
          <label className="inline">
            <input name="active" type="checkbox" defaultChecked={nature?.active ?? true} /> Ativa
          </label>
        </div>
      </section>

      {isProduct && (
        <>
          <section className="form-section" aria-labelledby="nature-cfop">
            <HelpTitle id="nature-cfop" title="CFOP">
              <p>
                Quatro dígitos. O CFOP de dentro do estado começa com 5 e o de outro estado, com 6. O produto não traz
                tabela de CFOP: confirme o código certo da operação com o seu contador.
              </p>
            </HelpTitle>
            <div className="field-grid">
              <label>
                CFOP dentro do estado
                <input name="cfop_in_state" required inputMode="numeric" maxLength={4} autoComplete="off" placeholder="5102" defaultValue={nature?.cfop_in_state ?? ""} />
              </label>
              <label>
                CFOP interestadual
                <input name="cfop_interstate" required inputMode="numeric" maxLength={4} autoComplete="off" placeholder="6102" defaultValue={nature?.cfop_interstate ?? ""} />
              </label>
            </div>
          </section>

          <section className="form-section" aria-labelledby="nature-rules">
            <HelpTitle id="nature-rules" title="Regras de tributação">
              <p>
                Os códigos e as alíquotas que vão nos itens desta natureza, por regime e, se quiser, por início do NCM
                do produto (a regra com o prefixo mais longo vale). ICMS: CSOSN de 3 dígitos no Simples Nacional, CST de
                2 dígitos no Lucro Presumido; PIS e COFINS: CST de 2 dígitos. Alíquotas em %, com até duas casas, e em
                branco quando não se aplicam. Só o formato é conferido aqui: os valores são do contador.
              </p>
            </HelpTitle>
            {rows.length === 0 && <p className="muted">Nenhuma regra ainda.</p>}
            {rows.map((row, index) => (
              <fieldset key={row.key} className="tax-rule">
                <legend>Regra {index + 1}</legend>
                <div className="field-grid">
                  <label>
                    Regime
                    <select
                      aria-label={`Regime da regra ${index + 1}`}
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
                    Início do NCM
                    <input
                      aria-label={`Início do NCM da regra ${index + 1}`}
                      inputMode="numeric"
                      maxLength={8}
                      autoComplete="off"
                      placeholder="Todos os produtos"
                      value={row.ncm_prefix}
                      onChange={(event) => change(row.key, { ncm_prefix: event.target.value })}
                    />
                  </label>
                  <label>
                    {row.regime === "simples_nacional" ? "CSOSN do ICMS" : "CST do ICMS"}
                    <input
                      aria-label={`ICMS da regra ${index + 1}`}
                      inputMode="numeric"
                      maxLength={3}
                      autoComplete="off"
                      value={row.icms_code}
                      onChange={(event) => change(row.key, { icms_code: event.target.value })}
                    />
                  </label>
                  <label>
                    Alíquota do ICMS (%)
                    <input
                      aria-label={`Alíquota do ICMS da regra ${index + 1}`}
                      inputMode="decimal"
                      autoComplete="off"
                      value={row.icms_rate}
                      onChange={(event) => change(row.key, { icms_rate: event.target.value })}
                    />
                  </label>
                  <label>
                    CST do PIS
                    <input
                      aria-label={`PIS da regra ${index + 1}`}
                      inputMode="numeric"
                      maxLength={2}
                      autoComplete="off"
                      value={row.pis_code}
                      onChange={(event) => change(row.key, { pis_code: event.target.value })}
                    />
                  </label>
                  <label>
                    Alíquota do PIS (%)
                    <input
                      aria-label={`Alíquota do PIS da regra ${index + 1}`}
                      inputMode="decimal"
                      autoComplete="off"
                      value={row.pis_rate}
                      onChange={(event) => change(row.key, { pis_rate: event.target.value })}
                    />
                  </label>
                  <label>
                    CST da COFINS
                    <input
                      aria-label={`COFINS da regra ${index + 1}`}
                      inputMode="numeric"
                      maxLength={2}
                      autoComplete="off"
                      value={row.cofins_code}
                      onChange={(event) => change(row.key, { cofins_code: event.target.value })}
                    />
                  </label>
                  <label>
                    Alíquota da COFINS (%)
                    <input
                      aria-label={`Alíquota da COFINS da regra ${index + 1}`}
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
                    aria-label={`Remover regra ${index + 1}`}
                    onClick={() => {
                      markDirty();
                      setRows((current) => current.filter((item) => item.key !== row.key));
                    }}
                  >
                    Remover
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
                Adicionar regra
              </Button>
            </div>
          </section>
        </>
      )}

      <div className="form-footer">
        <BackButton onClick={onCancel} />
        <Button type="submit" icon={nature === null ? PlusIcon : CheckIcon}>
          {nature === null ? "Adicionar natureza" : "Concluir natureza"}
        </Button>
      </div>
    </form>
  );
}
