import { useEffect, useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import {
  api,
  errorMessage,
  type FiscalEnvironment,
  type FiscalSettings,
  type OperationNature,
  type TaxRegime,
} from "../api";
import { Button } from "../components/Button";
import { FiscalSettingsHistory } from "../components/FiscalSettingsHistory";
import { HelpTitle } from "../components/HelpTitle";
import { CheckIcon } from "../components/icons/check";
import { PlusIcon } from "../components/icons/plus";
import { SquarePenIcon } from "../components/icons/square-pen";
import { NatureForm, REGIME_LABELS } from "../components/NatureForm";
import { Notice } from "../components/Notice";
import { STATES } from "../components/PartyForm";
import { PageHeader } from "../components/PageHeader";
import { Sheet } from "../components/Sheet";
import { Spinner } from "../components/Spinner";
import { StatusBadge } from "../components/StatusBadge";
import { useT } from "../i18n";
import { formatDate } from "../lib/entryInstallments";
import { certificateStatus } from "../lib/fiscalCertificate";
import { todayIso } from "../lib/settlement";
import { useCanWrite } from "../lib/useCanWrite";
import { useLastValue } from "../lib/useLastValue";

/** The settings as they are typed: every field is text until Salvar sends them. */
interface Form {
  legal_name: string;
  cnpj: string;
  state_registration: string;
  municipal_registration: string;
  postal_code: string;
  street: string;
  number: string;
  complement: string;
  district: string;
  city: string;
  state: string;
  regime: TaxRegime;
  environment: FiscalEnvironment;
  nfe_series: string;
  nfce_series: string;
  nfse_series: string;
  certificate_valid_until: string;
  city_ibge_code: string;
  natures: OperationNature[];
}

/** What the company has before it saves anything: the Simples Nacional, in homologation, series 1. */
const EMPTY: Form = {
  legal_name: "",
  cnpj: "",
  state_registration: "",
  municipal_registration: "",
  postal_code: "",
  street: "",
  number: "",
  complement: "",
  district: "",
  city: "",
  state: "",
  regime: "simples_nacional",
  environment: "homologation",
  nfe_series: "1",
  nfce_series: "1",
  nfse_series: "1",
  certificate_valid_until: "",
  city_ibge_code: "",
  natures: [],
};

function formOf(settings: FiscalSettings | null): Form {
  if (settings === null) return EMPTY;
  const { address } = settings;
  return {
    legal_name: settings.legal_name,
    cnpj: settings.cnpj,
    state_registration: settings.state_registration ?? "",
    municipal_registration: settings.municipal_registration ?? "",
    postal_code: address.postal_code,
    street: address.street,
    number: address.number ?? "",
    complement: address.complement ?? "",
    district: address.district ?? "",
    city: address.city,
    state: address.state,
    regime: settings.regime,
    environment: settings.environment,
    nfe_series: String(settings.nfe_series),
    nfce_series: String(settings.nfce_series),
    nfse_series: String(settings.nfse_series),
    certificate_valid_until: settings.certificate_valid_until ?? "",
    city_ibge_code: settings.city_ibge_code ?? "",
    natures: settings.natures,
  };
}

const optional = (text: string): string | null => text.trim() || null;
const SERIES = /^\d{1,3}$/;

function settingsOf(form: Form): FiscalSettings {
  return {
    legal_name: form.legal_name.trim(),
    cnpj: form.cnpj.trim(),
    state_registration: optional(form.state_registration),
    municipal_registration: optional(form.municipal_registration),
    address: {
      postal_code: form.postal_code.trim(),
      street: form.street.trim(),
      number: optional(form.number),
      complement: optional(form.complement),
      district: optional(form.district),
      city: form.city.trim(),
      state: form.state,
    },
    regime: form.regime,
    environment: form.environment,
    nfe_series: Number(form.nfe_series),
    nfce_series: Number(form.nfce_series),
    nfse_series: Number(form.nfse_series),
    certificate_valid_until: optional(form.certificate_valid_until),
    city_ibge_code: optional(form.city_ibge_code),
    natures: form.natures,
  };
}

/** The certificate's standing, in words: what the product knows is only the day the person typed. */
function CertificateStatusNotice({ validUntil, today }: { validUntil: string; today: string }) {
  const t = useT();
  if (validUntil === "") return null;
  const status = certificateStatus(validUntil, today);
  const day = formatDate(validUntil);
  if (status.kind === "expired") {
    return <Notice message={t("Certificate expired on {day}: no document can be issued until it is renewed.", { day })} />;
  }
  if (status.kind === "expiring") {
    const message =
      status.daysLeft === 0
        ? t("Certificate expires today ({day}). Renew it in the gateway's panel.", { day })
        : status.daysLeft === 1
          ? t("Certificate expires tomorrow ({day}). Renew it in the gateway's panel.", { day })
          : t("Certificate expires in {days} days ({day}). Renew it in the gateway's panel.", { days: status.daysLeft, day });
    return <Notice tone="warning" message={message} />;
  }
  if (status.kind === "valid") {
    return <p className="muted">{t("Expires in {days} days ({day}).", { days: status.daysLeft, day })}</p>;
  }
  return null;
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function FiscalSettingsPage() {
  const t = useT();
  const kindLabel = (nature: OperationNature) => (nature.kind === "product" ? t("Goods") : t("Service"));
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("fiscal");
  // The server reads no clock, so how close the certificate is to expiring is told with the day where the person is.
  const [today] = useState(() => todayIso());
  const [loaded, setLoaded] = useState(false);
  const [stored, setStored] = useState<FiscalSettings | null>(null);
  // Which gateway issues, `null` for none, and unknown (no notice) while it is not read or cannot be.
  const [gateway, setGateway] = useState<string | null | undefined>(undefined);
  const [form, setForm] = useState<Form>(EMPTY);
  const [edited, setEdited] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // The nature in the sheet: its place in the list, "new" for one being added, or none.
  const [editing, setEditing] = useState<number | "new" | null>(null);
  const shown = useLastValue(editing);
  // Bumped after a save so the trail reads again.
  const [trail, setTrail] = useState(0);

  useEffect(() => {
    let current = true;
    api
      .getFiscalSettings(companyId)
      .then((settings) => {
        if (!current) return;
        setStored(settings);
        setForm(formOf(settings));
        setLoaded(true);
      })
      .catch((failure: unknown) => current && setError(errorMessage(failure)));
    return () => {
      current = false;
    };
  }, [companyId]);

  useEffect(() => {
    let current = true;
    api
      .getFiscalGateway(companyId)
      .then((answer) => current && setGateway(answer.gateway))
      .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [companyId]);

  function set<K extends keyof Form>(field: K, value: Form[K]) {
    setForm((current) => ({ ...current, [field]: value }));
    setEdited(true);
    setSaved(null);
  }

  function closeSheet() {
    setEditing(null);
  }

  function keepNature(nature: OperationNature) {
    setForm((current) => ({
      ...current,
      natures:
        editing === "new" || editing === null
          ? [...current.natures, nature]
          : current.natures.map((item, index) => (index === editing ? nature : item)),
    }));
    setEdited(true);
    setSaved(null);
    setEditing(null);
  }

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (saving) return;
    if (![form.nfe_series, form.nfce_series, form.nfse_series].every((series) => SERIES.test(series.trim()))) {
      setError(t("Give the series as numbers, from 1 to 999."));
      return;
    }
    setSaving(true);
    setError(null);
    setSaved(null);
    try {
      const settings = await api.replaceFiscalSettings(companyId, settingsOf(form));
      setStored(settings);
      setForm(formOf(settings));
      setEdited(false);
      setTrail((count) => count + 1);
      setSaved(t("Fiscal settings saved."));
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  const nature = typeof shown === "number" ? (form.natures[shown] ?? null) : null;

  return (
    <>
      <PageHeader
        title={t("Fiscal settings")}
        description={t(
          "Who issues the fiscal documents, the tax regime, the gateway environment, the series, the day the A1 certificate expires and the operation natures with the tax codes of each.",
        )}
      />
      <Notice message={error} />
      <Notice message={saved} tone="success" />
      {!loaded && error === null ? (
        <p role="status" className="status">
          <Spinner /> {t("Loading…")}
        </p>
      ) : (
        loaded && (
          <>
            {gateway === null && (
              <Notice
                tone="warning"
                message={t("No fiscal gateway is configured in this installation (variable FISCAL_GATEWAY): documents cannot be issued.")}
              />
            )}
            {gateway === "fake" && (
              <Notice
                tone="warning"
                message={t("The gateway is the test one (fake): documents are fake and have no fiscal value.")}
              />
            )}
            {stored === null && (
              <Notice
                tone="warning"
                message={t("These settings have not been saved yet: without them no fiscal document can be issued.")}
              />
            )}
            <form className="record-form" onSubmit={(event) => void save(event)}>
              <fieldset className="fieldset-plain" disabled={!canWrite}>
                <section className="form-section" aria-labelledby="fiscal-issuer">
                  <HelpTitle id="fiscal-issuer" title={t("Issuer")}>
                    <p>
                      {t(
                        "The data of the company that issues the invoices, as in the CNPJ registry. Only the format is checked here; the state registration can be ISENTO. The municipality's IBGE code (seven digits, IBGE table) is what the NFS-e says about where it was issued: without it no NFS-e can be issued.",
                      )}
                    </p>
                  </HelpTitle>
                  <div className="field-grid">
                    <label>
                      {t("Legal name")}
                      <input required maxLength={150} autoComplete="off" value={form.legal_name} onChange={(event) => set("legal_name", event.target.value)} />
                    </label>
                    <label>
                      CNPJ
                      <input required inputMode="text" maxLength={18} autoComplete="off" placeholder="00.000.000/0000-00" value={form.cnpj} onChange={(event) => set("cnpj", event.target.value)} />
                    </label>
                    <label>
                      {t("State registration")}
                      <input maxLength={20} autoComplete="off" placeholder={t("Numbers or ISENTO")} value={form.state_registration} onChange={(event) => set("state_registration", event.target.value)} />
                    </label>
                    <label>
                      {t("Municipal registration")}
                      <input maxLength={20} autoComplete="off" value={form.municipal_registration} onChange={(event) => set("municipal_registration", event.target.value)} />
                    </label>
                  </div>
                  <fieldset className="address-fields">
                    <legend>{t("Address")}</legend>
                    <label className="span-6">
                      {t("Postal code")}
                      <input required inputMode="numeric" autoComplete="off" placeholder="00000-000" maxLength={9} value={form.postal_code} onChange={(event) => set("postal_code", event.target.value)} />
                    </label>
                    <label className="span-4">
                      {t("Street")}
                      <input required maxLength={150} autoComplete="off" value={form.street} onChange={(event) => set("street", event.target.value)} />
                    </label>
                    <label className="span-2">
                      {t("Number")}
                      <input maxLength={20} autoComplete="off" value={form.number} onChange={(event) => set("number", event.target.value)} />
                    </label>
                    <label className="span-3">
                      {t("Complement")}
                      <input maxLength={80} autoComplete="off" value={form.complement} onChange={(event) => set("complement", event.target.value)} />
                    </label>
                    <label className="span-3">
                      {t("District")}
                      <input maxLength={80} autoComplete="off" value={form.district} onChange={(event) => set("district", event.target.value)} />
                    </label>
                    <label className="span-4">
                      {t("City")}
                      <input required maxLength={80} autoComplete="off" value={form.city} onChange={(event) => set("city", event.target.value)} />
                    </label>
                    <label className="span-2">
                      {t("Issuer state")}
                      <select required value={form.state} onChange={(event) => set("state", event.target.value)}>
                        <option value=""></option>
                        {STATES.map((state) => (
                          <option key={state} value={state}>
                            {state}
                          </option>
                        ))}
                      </select>
                    </label>
                  </fieldset>
                  <div className="field-grid">
                    <label>
                      {t("Municipality IBGE code")}
                      <input
                        inputMode="numeric"
                        maxLength={7}
                        pattern="[0-9]{7}"
                        autoComplete="off"
                        placeholder="3550308"
                        value={form.city_ibge_code}
                        onChange={(event) => set("city_ibge_code", event.target.value)}
                      />
                    </label>
                  </div>
                </section>

                <section className="form-section" aria-labelledby="fiscal-regime">
                  <HelpTitle id="fiscal-regime" title={t("Regime and environment")}>
                    <p>
                      {t(
                        "The regime decides which tax codes the rules use (MEI, Simples with sublimit excess and Lucro Real are not accepted yet). In the test environment the documents are test ones and have no fiscal validity; production requires the day the certificate expires.",
                      )}
                    </p>
                  </HelpTitle>
                  <div className="field-grid">
                    <label>
                      {t("Tax regime")}
                      <select value={form.regime} onChange={(event) => set("regime", event.target.value as TaxRegime)}>
                        {(Object.keys(REGIME_LABELS) as TaxRegime[]).map((key) => (
                          <option key={key} value={key}>
                            {REGIME_LABELS[key]}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      {t("Environment")}
                      <select value={form.environment} onChange={(event) => set("environment", event.target.value as FiscalEnvironment)}>
                        <option value="homologation">{t("Test (sandbox)")}</option>
                        <option value="production">{t("Production")}</option>
                      </select>
                    </label>
                  </div>
                </section>

                <section className="form-section" aria-labelledby="fiscal-series">
                  <HelpTitle id="fiscal-series" title={t("Series")}>
                    <p>
                      {t("The series of each kind of document, from 1 to 999. The number of each invoice is the gateway's: the product keeps no counter.")}
                    </p>
                  </HelpTitle>
                  <div className="field-grid">
                    <label>
                      {t("NF-e series")}
                      <input required inputMode="numeric" maxLength={3} autoComplete="off" value={form.nfe_series} onChange={(event) => set("nfe_series", event.target.value)} />
                    </label>
                    <label>
                      {t("NFC-e series")}
                      <input required inputMode="numeric" maxLength={3} autoComplete="off" value={form.nfce_series} onChange={(event) => set("nfce_series", event.target.value)} />
                    </label>
                    <label>
                      {t("NFS-e series")}
                      <input required inputMode="numeric" maxLength={3} autoComplete="off" value={form.nfse_series} onChange={(event) => set("nfse_series", event.target.value)} />
                    </label>
                  </div>
                </section>

                <section className="form-section" aria-labelledby="fiscal-certificate">
                  <HelpTitle id="fiscal-certificate" title={t("A1 certificate")}>
                    <p>
                      {t(
                        "The certificate and its password are not sent to this product: they are sent in the issuing gateway's panel. Only the day it expires is kept here, which you read from the certificate itself, to warn beforehand and prevent issuing after it.",
                      )}
                    </p>
                  </HelpTitle>
                  <div className="field-grid">
                    <label>
                      {t("The certificate expires on")}
                      <input type="date" value={form.certificate_valid_until} onChange={(event) => set("certificate_valid_until", event.target.value)} />
                    </label>
                  </div>
                  <CertificateStatusNotice validUntil={form.certificate_valid_until} today={today} />
                </section>

                <section className="form-section" aria-labelledby="fiscal-natures">
                  <HelpTitle id="fiscal-natures" title={t("Operation natures")}>
                    <p>
                      {t(
                        "What the company calls each operation (sale, sale for delivery, service), with the CFOP of the goods and the tax rules of the items. The product has no tax table: the codes and rates are yours and your accountant's.",
                      )}
                    </p>
                  </HelpTitle>
                  {form.natures.length === 0 ? (
                    <p className="muted">{t("No natures yet: without one there is nothing to choose when issuing.")}</p>
                  ) : (
                    <div className="table-wrap">
                      <table>
                        <thead>
                          <tr>
                            <th>{t("Name")}</th>
                            <th>{t("Code")}</th>
                            <th>{t("Type")}</th>
                            <th>CFOP</th>
                            <th>{t("Rules")}</th>
                            <th>{t("Status")}</th>
                            <th>
                              <span className="visually-hidden">{t("Actions")}</span>
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          {form.natures.map((item, index) => (
                            <tr key={`${item.code}-${index}`}>
                              <td>{item.name}</td>
                              <td>{item.code}</td>
                              <td>{kindLabel(item)}</td>
                              <td>{item.kind === "product" ? `${item.cfop_in_state} / ${item.cfop_interstate}` : "—"}</td>
                              <td>{item.rules.length}</td>
                              <td>
                                <StatusBadge active={item.active} />
                              </td>
                              <td className="row-actions">
                                {canWrite && (
                                  <>
                                    <Button iconOnly icon={SquarePenIcon} type="button" variant="ghost" aria-label={t("Edit {name}", { name: item.name })} onClick={() => setEditing(index)}>
                                      {t("Change")}
                                    </Button>
                                    <Button
                                      type="button"
                                      variant="ghost"
                                      className="btn-small"
                                      aria-label={t("Remove {name}", { name: item.name })}
                                      onClick={() => {
                                        setForm((current) => ({ ...current, natures: current.natures.filter((_, at) => at !== index) }));
                                        setEdited(true);
                                        setSaved(null);
                                      }}
                                    >
                                      {t("Remove")}
                                    </Button>
                                  </>
                                )}
                              </td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                  {canWrite && (
                    <div className="form-actions">
                      <Button type="button" variant="ghost" icon={PlusIcon} onClick={() => setEditing("new")}>
                        {t("Add nature")}
                      </Button>
                    </div>
                  )}
                </section>
              </fieldset>

              {canWrite && (
                <div className="form-footer">
                  <span className="muted" role="status">
                    {edited ? t("There are changes not saved yet.") : ""}
                  </span>
                  <Button type="submit" icon={CheckIcon} pending={saving}>
                    {t("Save")}
                  </Button>
                </div>
              )}
            </form>

            <section className="card">
              <HelpTitle level={2} title={t("History")}>
                <p>{t("What changed in these settings, by whom and when, from the most recent to the oldest.")}</p>
              </HelpTitle>
              <FiscalSettingsHistory key={trail} companyId={companyId} />
            </section>
          </>
        )
      )}
      {/* Outside the form: a portal's events reach the parents of the React tree, and its submit must not be this one's. */}
      {canWrite && (
        <Sheet
          open={editing !== null}
          title={editing === "new" ? t("New nature") : t("Edit {name}", { name: nature?.name ?? "" })}
          onClose={closeSheet}
        >
          {shown !== null && (
            <NatureForm
              key={shown === "new" ? "new" : (nature?.code ?? "none")}
              nature={shown === "new" ? null : nature}
              regime={form.regime}
              onSubmit={keepNature}
              onCancel={closeSheet}
            />
          )}
        </Sheet>
      )}
    </>
  );
}
