import { useEffect, useState, type FormEvent } from "react";
import { api, errorMessage, type IssueServiceChoices, type OperationNature, type Sale, type SpecialTaxRegime } from "../api";
import { formatMoney } from "../lib/decimal";
import { SPECIAL_REGIME_LABELS } from "../lib/fiscalDocuments";
import { Button } from "./Button";
import { HelpTitle } from "./HelpTitle";
import { Notice } from "./Notice";

interface Props {
  companyId: string;
  natures: OperationNature[];
  /** The sale is fixed (sending a rejected document again) instead of chosen. */
  fixedSale?: string;
  submitLabel: string;
  onSubmit: (choices: IssueServiceChoices) => Promise<void>;
}

/**
 * The NFS-e is issued from the service lines of a sale under a service nature whose ISSQN terms the company wrote in the
 * settings. What is chosen here is what the sale does not say and the product does not know, so none starts filled
 * (ADR 0081). Nothing is sent until the button.
 */
export function ServiceIssueForm({ companyId, natures, fixedSale, submitLabel, onSubmit }: Props) {
  const [sales, setSales] = useState<Sale[]>([]);
  const [sale, setSale] = useState(fixedSale ?? "");
  const [nature, setNature] = useState("");
  const [competence, setCompetence] = useState("");
  const [city, setCity] = useState("");
  const [regime, setRegime] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    if (fixedSale) return;
    let current = true;
    api
      .listSales(companyId, { q: "", status: "completed", sort: "-number" })
      .then((page) => current && setSales(page.items))
      .catch((failure) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId, fixedSale]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    try {
      await onSubmit({
        sale_id: sale,
        nature_code: nature,
        competence,
        service_city_code: city.trim(),
        special_regime: regime as SpecialTaxRegime,
      });
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setPending(false);
    }
  }

  // Only a service nature with its ISSQN terms can issue: one without them is a service the company has not described.
  const serviceNatures = natures.filter((item) => item.kind === "service" && item.active && item.service !== null);

  return (
    <form className="record-form" onSubmit={(event) => void submit(event)}>
      <HelpTitle title="Como emitir" level={3}>
        <p>
          A NFS-e sai das linhas de serviço da venda. O código do serviço, a forma de cobrança do ISS e a alíquota vêm da
          natureza de operação, que a empresa e o contador escrevem nas configurações fiscais. Escolha aqui o que a venda não
          diz: o dia de competência, o município onde o serviço foi prestado (pelo código do IBGE, que decide de quem é o ISS e
          muitas vezes não é o da empresa) e o regime especial de tributação. Só o Simples Nacional emite por enquanto.
        </p>
      </HelpTitle>
      <Notice message={error} />
      <div className="field-grid">
        {!fixedSale && (
          <label>
            Venda
            <select required value={sale} onChange={(event) => setSale(event.target.value)}>
              <option value="">Escolha a venda</option>
              {sales.map((item) => (
                <option key={item.id} value={item.id}>
                  {`Venda ${item.number} · ${formatMoney(item.total)}`}
                </option>
              ))}
            </select>
          </label>
        )}
        <label>
          Natureza de operação
          <select required value={nature} onChange={(event) => setNature(event.target.value)}>
            <option value="">Escolha a natureza</option>
            {serviceNatures.map((item) => (
              <option key={item.code} value={item.code}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Competência
          <input required type="date" value={competence} onChange={(event) => setCompetence(event.target.value)} />
        </label>
        <label>
          Município da prestação (código do IBGE)
          <input
            required
            pattern="[0-9]{7}"
            maxLength={7}
            value={city}
            onChange={(event) => setCity(event.target.value)}
            title="Sete dígitos, da tabela do IBGE"
          />
        </label>
        <label>
          Regime especial de tributação
          <select required value={regime} onChange={(event) => setRegime(event.target.value)}>
            <option value="">Escolha o regime</option>
            {Object.entries(SPECIAL_REGIME_LABELS).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </select>
        </label>
      </div>
      {serviceNatures.length === 0 && (
        <Notice
          tone="warning"
          message="Nenhuma natureza de serviço ativa tem os termos do ISS. Descreva-os nas configurações fiscais antes de emitir."
        />
      )}
      <Button type="submit" pending={pending}>
        {submitLabel}
      </Button>
    </form>
  );
}
