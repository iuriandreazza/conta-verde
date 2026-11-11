import { useEffect, useState, type FormEvent } from "react";
import { api, errorMessage, type IssueServiceChoices, type OperationNature, type Sale, type SpecialTaxRegime } from "../api";
import { useT } from "../i18n";
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
  const t = useT();
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
      <HelpTitle title={t("How to issue")} level={3}>
        <p>
          {t(
            "The NFS-e comes from the sale's service lines. The service code, how ISS is charged and the rate come from the operation nature, which the company and its accountant write in the fiscal settings. Choose here what the sale does not say: the competence day, the municipality where the service was rendered (by its IBGE code, which decides who gets the ISS and is often not the company's) and the special tax regime. Only the Simples Nacional issues for now.",
          )}
        </p>
      </HelpTitle>
      <Notice message={error} />
      <div className="field-grid">
        {!fixedSale && (
          <label>
            {t("Sale")}
            <select required value={sale} onChange={(event) => setSale(event.target.value)}>
              <option value="">{t("Choose the sale")}</option>
              {sales.map((item) => (
                <option key={item.id} value={item.id}>
                  {t("Sale {number} · {total}", { number: item.number, total: formatMoney(item.total) })}
                </option>
              ))}
            </select>
          </label>
        )}
        <label>
          {t("Operation nature")}
          <select required value={nature} onChange={(event) => setNature(event.target.value)}>
            <option value="">{t("Choose the nature")}</option>
            {serviceNatures.map((item) => (
              <option key={item.code} value={item.code}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("Competence")}
          <input required type="date" value={competence} onChange={(event) => setCompetence(event.target.value)} />
        </label>
        <label>
          {t("Municipality of the service (IBGE code)")}
          <input
            required
            pattern="[0-9]{7}"
            maxLength={7}
            value={city}
            onChange={(event) => setCity(event.target.value)}
            title={t("Seven digits, from the IBGE table")}
          />
        </label>
        <label>
          {t("Special tax regime")}
          <select required value={regime} onChange={(event) => setRegime(event.target.value)}>
            <option value="">{t("Choose the regime")}</option>
            {Object.entries(SPECIAL_REGIME_LABELS).map(([key, label]) => (
              <option key={key} value={key}>
                {t(label)}
              </option>
            ))}
          </select>
        </label>
      </div>
      {serviceNatures.length === 0 && (
        <Notice
          tone="warning"
          message={t("No active service nature has the ISS terms. Describe them in the fiscal settings before issuing.")}
        />
      )}
      <Button type="submit" pending={pending}>
        {submitLabel}
      </Button>
    </form>
  );
}
