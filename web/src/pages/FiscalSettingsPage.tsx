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
    natures: form.natures,
  };
}

const kindLabel = (nature: OperationNature) => (nature.kind === "product" ? "Mercadoria" : "Serviço");

/** The certificate's standing, in words: what the product knows is only the day the person typed. */
function CertificateStatusNotice({ validUntil, today }: { validUntil: string; today: string }) {
  if (validUntil === "") return null;
  const status = certificateStatus(validUntil, today);
  const day = formatDate(validUntil);
  if (status.kind === "expired") {
    return <Notice message={`Certificado vencido em ${day}: nenhum documento pode ser emitido até renová-lo.`} />;
  }
  if (status.kind === "expiring") {
    const when = status.daysLeft === 0 ? "vence hoje" : status.daysLeft === 1 ? "vence amanhã" : `vence em ${status.daysLeft} dias`;
    return <Notice tone="warning" message={`Certificado ${when} (${day}). Renove-o no painel do gateway.`} />;
  }
  if (status.kind === "valid") {
    return <p className="muted">{`Vence em ${status.daysLeft} dias (${day}).`}</p>;
  }
  return null;
}

// Layout remounts the page whenever the path changes, so `companyId` never changes within one instance.
export function FiscalSettingsPage() {
  const { companyId = "" } = useParams();
  const canWrite = useCanWrite("fiscal");
  // The server reads no clock, so how close the certificate is to expiring is told with the day where the person is.
  const [today] = useState(() => todayIso());
  const [loaded, setLoaded] = useState(false);
  const [stored, setStored] = useState<FiscalSettings | null>(null);
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
      setError("Informe as séries como números, de 1 a 999.");
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
      setSaved("Configurações fiscais salvas.");
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
        title="Configurações fiscais"
        description="Quem emite os documentos fiscais, o regime tributário, o ambiente do gateway, as séries, o dia em que o certificado A1 vence e as naturezas de operação com os códigos de imposto de cada uma."
      />
      <Notice message={error} />
      <Notice message={saved} tone="success" />
      {!loaded && error === null ? (
        <p role="status" className="status">
          <Spinner /> Carregando…
        </p>
      ) : (
        loaded && (
          <>
            {stored === null && (
              <Notice
                tone="warning"
                message="Estas configurações ainda não foram salvas: sem elas nenhum documento fiscal pode ser emitido."
              />
            )}
            <form className="record-form" onSubmit={(event) => void save(event)}>
              <fieldset className="fieldset-plain" disabled={!canWrite}>
                <section className="form-section" aria-labelledby="fiscal-issuer">
                  <HelpTitle id="fiscal-issuer" title="Emitente">
                    <p>
                      Os dados da empresa que emite as notas, como constam no cadastro do CNPJ. Só o formato é conferido
                      aqui; a inscrição estadual pode ser ISENTO.
                    </p>
                  </HelpTitle>
                  <div className="field-grid">
                    <label>
                      Razão social
                      <input required maxLength={150} autoComplete="off" value={form.legal_name} onChange={(event) => set("legal_name", event.target.value)} />
                    </label>
                    <label>
                      CNPJ
                      <input required inputMode="text" maxLength={18} autoComplete="off" placeholder="00.000.000/0000-00" value={form.cnpj} onChange={(event) => set("cnpj", event.target.value)} />
                    </label>
                    <label>
                      Inscrição estadual
                      <input maxLength={20} autoComplete="off" placeholder="Números ou ISENTO" value={form.state_registration} onChange={(event) => set("state_registration", event.target.value)} />
                    </label>
                    <label>
                      Inscrição municipal
                      <input maxLength={20} autoComplete="off" value={form.municipal_registration} onChange={(event) => set("municipal_registration", event.target.value)} />
                    </label>
                  </div>
                  <fieldset className="address-fields">
                    <legend>Endereço</legend>
                    <label className="span-6">
                      CEP
                      <input required inputMode="numeric" autoComplete="off" placeholder="00000-000" maxLength={9} value={form.postal_code} onChange={(event) => set("postal_code", event.target.value)} />
                    </label>
                    <label className="span-4">
                      Logradouro
                      <input required maxLength={150} autoComplete="off" value={form.street} onChange={(event) => set("street", event.target.value)} />
                    </label>
                    <label className="span-2">
                      Número
                      <input maxLength={20} autoComplete="off" value={form.number} onChange={(event) => set("number", event.target.value)} />
                    </label>
                    <label className="span-3">
                      Complemento
                      <input maxLength={80} autoComplete="off" value={form.complement} onChange={(event) => set("complement", event.target.value)} />
                    </label>
                    <label className="span-3">
                      Bairro
                      <input maxLength={80} autoComplete="off" value={form.district} onChange={(event) => set("district", event.target.value)} />
                    </label>
                    <label className="span-4">
                      Cidade
                      <input required maxLength={80} autoComplete="off" value={form.city} onChange={(event) => set("city", event.target.value)} />
                    </label>
                    <label className="span-2">
                      UF
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
                </section>

                <section className="form-section" aria-labelledby="fiscal-regime">
                  <HelpTitle id="fiscal-regime" title="Regime e ambiente">
                    <p>
                      O regime decide quais códigos de imposto as regras usam (MEI, Simples com excesso de sublimite e
                      Lucro Real ainda não são aceitos). Em homologação os documentos são de teste e não têm validade
                      fiscal; a produção exige o dia em que o certificado vence.
                    </p>
                  </HelpTitle>
                  <div className="field-grid">
                    <label>
                      Regime tributário
                      <select value={form.regime} onChange={(event) => set("regime", event.target.value as TaxRegime)}>
                        {(Object.keys(REGIME_LABELS) as TaxRegime[]).map((key) => (
                          <option key={key} value={key}>
                            {REGIME_LABELS[key]}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      Ambiente
                      <select value={form.environment} onChange={(event) => set("environment", event.target.value as FiscalEnvironment)}>
                        <option value="homologation">Homologação (teste)</option>
                        <option value="production">Produção</option>
                      </select>
                    </label>
                  </div>
                </section>

                <section className="form-section" aria-labelledby="fiscal-series">
                  <HelpTitle id="fiscal-series" title="Séries">
                    <p>
                      A série de cada tipo de documento, de 1 a 999. O número de cada nota é do gateway: o produto não
                      guarda contador.
                    </p>
                  </HelpTitle>
                  <div className="field-grid">
                    <label>
                      Série da NF-e
                      <input required inputMode="numeric" maxLength={3} autoComplete="off" value={form.nfe_series} onChange={(event) => set("nfe_series", event.target.value)} />
                    </label>
                    <label>
                      Série da NFC-e
                      <input required inputMode="numeric" maxLength={3} autoComplete="off" value={form.nfce_series} onChange={(event) => set("nfce_series", event.target.value)} />
                    </label>
                    <label>
                      Série da NFS-e
                      <input required inputMode="numeric" maxLength={3} autoComplete="off" value={form.nfse_series} onChange={(event) => set("nfse_series", event.target.value)} />
                    </label>
                  </div>
                </section>

                <section className="form-section" aria-labelledby="fiscal-certificate">
                  <HelpTitle id="fiscal-certificate" title="Certificado A1">
                    <p>
                      O certificado e a senha dele não são enviados a este produto: eles são enviados no painel do
                      gateway de emissão. Aqui fica só o dia em que ele vence, que você lê no próprio certificado, para
                      avisar antes e impedir emissões depois dele.
                    </p>
                  </HelpTitle>
                  <div className="field-grid">
                    <label>
                      O certificado vence em
                      <input type="date" value={form.certificate_valid_until} onChange={(event) => set("certificate_valid_until", event.target.value)} />
                    </label>
                  </div>
                  <CertificateStatusNotice validUntil={form.certificate_valid_until} today={today} />
                </section>

                <section className="form-section" aria-labelledby="fiscal-natures">
                  <HelpTitle id="fiscal-natures" title="Naturezas de operação">
                    <p>
                      O que a empresa chama de cada operação (venda, venda para entrega, serviço), com o CFOP das
                      mercadorias e as regras de imposto dos itens. O produto não traz tabela de imposto: os códigos e as
                      alíquotas são seus e do seu contador.
                    </p>
                  </HelpTitle>
                  {form.natures.length === 0 ? (
                    <p className="muted">Nenhuma natureza ainda: sem uma, não há o que escolher ao emitir.</p>
                  ) : (
                    <div className="table-wrap">
                      <table>
                        <thead>
                          <tr>
                            <th>Nome</th>
                            <th>Código</th>
                            <th>Tipo</th>
                            <th>CFOP</th>
                            <th>Regras</th>
                            <th>Situação</th>
                            <th>
                              <span className="visually-hidden">Ações</span>
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
                                    <Button iconOnly icon={SquarePenIcon} type="button" variant="ghost" aria-label={`Alterar ${item.name}`} onClick={() => setEditing(index)}>
                                      Alterar
                                    </Button>
                                    <Button
                                      type="button"
                                      variant="ghost"
                                      className="btn-small"
                                      aria-label={`Remover ${item.name}`}
                                      onClick={() => {
                                        setForm((current) => ({ ...current, natures: current.natures.filter((_, at) => at !== index) }));
                                        setEdited(true);
                                        setSaved(null);
                                      }}
                                    >
                                      Remover
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
                        Adicionar natureza
                      </Button>
                    </div>
                  )}
                </section>
              </fieldset>

              {canWrite && (
                <div className="form-footer">
                  <span className="muted" role="status">
                    {edited ? "Há alterações ainda não salvas." : ""}
                  </span>
                  <Button type="submit" icon={CheckIcon} pending={saving}>
                    Salvar
                  </Button>
                </div>
              )}
            </form>

            <section className="card">
              <HelpTitle level={2} title="Histórico">
                <p>O que mudou nestas configurações, por quem e quando, do mais recente ao mais antigo.</p>
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
          title={editing === "new" ? "Nova natureza" : `Alterar ${nature?.name ?? ""}`}
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
