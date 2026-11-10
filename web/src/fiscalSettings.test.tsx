import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, vi } from "vitest";
import type { FiscalSettings, HistoryEntry } from "./api";
import { certificateStatus } from "./lib/fiscalCertificate";
import { OWNER_PERMISSIONS, mockApi, profile, renderAt } from "./testSupport";

beforeEach(() => {
  vi.restoreAllMocks();
  // The certificate's standing is told from today's day where the person is.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(2026, 10, 15, 12));
});
afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const stored: FiscalSettings = {
  legal_name: "Verde Comércio Ltda",
  cnpj: "11222333000181",
  state_registration: "110042490114",
  municipal_registration: null,
  address: {
    postal_code: "01310100",
    street: "Avenida Paulista",
    number: "1000",
    complement: null,
    district: "Bela Vista",
    city: "São Paulo",
    state: "SP",
  },
  regime: "simples_nacional",
  environment: "homologation",
  nfe_series: 1,
  nfce_series: 1,
  nfse_series: 1,
  certificate_valid_until: "2027-03-31",
  city_ibge_code: null,
  natures: [
    {
      code: "venda",
      name: "Venda de mercadoria",
      kind: "product",
      cfop_in_state: "5102",
      cfop_interstate: "6102",
      active: true,
      rules: [
        {
          regime: "simples_nacional",
          ncm_prefix: null,
          icms_code: "102",
          pis_code: "07",
          cofins_code: "07",
          icms_rate: null,
          pis_rate: null,
          cofins_rate: null,
        },
      ],
      service: null,
    },
  ],
};

const trail: HistoryEntry[] = [
  {
    at: Date.UTC(2026, 10, 2, 15, 0),
    actor: { kind: "user", id: "u1", name: "Ana" },
    action: "created",
    changes: [{ field: "regime", from: null, to: "simples_nacional" }],
  },
];

/** What the API says about the settings; a PUT answers with what it was asked. */
function serve(settings: FiscalSettings | null, me = profile) {
  return mockApi((url, init) => {
    if (url === "/api/auth/me") return { status: 200, body: me };
    if (url.endsWith("/fiscal/settings/history")) return { status: 200, body: trail };
    if (url.endsWith("/fiscal/settings") && init?.method === "PUT") return { status: 200, body: JSON.parse(String(init.body)) };
    if (url.endsWith("/fiscal/settings")) return { status: 200, body: settings };
    return { status: 404, body: { error: "not found" } };
  });
}

const putsOf = (fetchMock: ReturnType<typeof mockApi>) =>
  fetchMock.mock.calls.filter(([, init]) => init?.method === "PUT").map(([, init]) => JSON.parse(String(init?.body)));

const withPermission = (fiscal: "read" | "write") => ({
  ...profile,
  companies: [{ id: "c1", name: "Padaria", role: "viewer" as const, permissions: { ...OWNER_PERMISSIONS, fiscal } }],
});

it("starts empty with the defaults and says nothing can be issued until it is saved", async () => {
  serve(null);
  renderAt("/companies/c1/fiscal/settings");

  expect(await screen.findByText(/ainda não foram salvas/)).toBeInTheDocument();
  expect(screen.getByLabelText("Regime tributário")).toHaveValue("simples_nacional");
  expect(screen.getByLabelText("Ambiente")).toHaveValue("homologation");
  for (const label of ["Série da NF-e", "Série da NFC-e", "Série da NFS-e"]) {
    expect(screen.getByLabelText(label)).toHaveValue("1");
  }
  expect(screen.getByText(/Nenhuma natureza ainda/)).toBeInTheDocument();
});

it("never asks for the certificate or its password: only the day it expires", async () => {
  serve(null);
  renderAt("/companies/c1/fiscal/settings");
  await screen.findByText(/ainda não foram salvas/);

  expect(document.querySelector('input[type="file"], input[type="password"]')).toBeNull();
  expect(screen.getByLabelText("O certificado vence em")).toHaveAttribute("type", "date");
});

it("saves everything that was typed in one PUT, as the API names it", async () => {
  const fetchMock = serve(null);
  renderAt("/companies/c1/fiscal/settings");
  await screen.findByText(/ainda não foram salvas/);

  const type = (label: string, text: string) => userEvent.type(screen.getByLabelText(label), text);
  await type("Razão social", "Verde Comércio Ltda");
  await type("CNPJ", "11.222.333/0001-81");
  await type("Inscrição estadual", "ISENTO");
  await type("CEP", "01310-100");
  await type("Logradouro", "Avenida Paulista");
  await type("Número", "1000");
  await type("Cidade", "São Paulo");
  await userEvent.selectOptions(screen.getByLabelText("UF"), "SP");
  await type("Código IBGE do município", "3550308");
  await userEvent.selectOptions(screen.getByLabelText("Regime tributário"), "lucro_presumido");
  await userEvent.selectOptions(screen.getByLabelText("Ambiente"), "production");
  await userEvent.clear(screen.getByLabelText("Série da NF-e"));
  await type("Série da NF-e", "2");
  await userEvent.type(screen.getByLabelText("O certificado vence em"), "2027-03-31");
  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));

  expect(await screen.findByText("Configurações fiscais salvas.")).toBeInTheDocument();
  expect(putsOf(fetchMock)).toEqual([
    {
      legal_name: "Verde Comércio Ltda",
      cnpj: "11.222.333/0001-81",
      state_registration: "ISENTO",
      municipal_registration: null,
      address: {
        postal_code: "01310-100",
        street: "Avenida Paulista",
        number: "1000",
        complement: null,
        district: null,
        city: "São Paulo",
        state: "SP",
      },
      regime: "lucro_presumido",
      environment: "production",
      nfe_series: 2,
      nfce_series: 1,
      nfse_series: 1,
      certificate_valid_until: "2027-03-31",
      city_ibge_code: "3550308",
      natures: [],
    },
  ]);
});

it("adds a product nature with a rule in the sheet, shows it in the table and sends it with the settings", async () => {
  const fetchMock = serve({ ...stored, natures: [] });
  renderAt("/companies/c1/fiscal/settings");
  await screen.findByDisplayValue("Verde Comércio Ltda");

  await userEvent.click(screen.getByRole("button", { name: "Adicionar natureza" }));
  const sheet = await screen.findByRole("dialog", { name: "Nova natureza" });
  const inSheet = within(sheet);
  await userEvent.type(inSheet.getByLabelText("Nome"), "Venda de mercadoria");
  await userEvent.type(inSheet.getByLabelText("Código"), "venda");
  await userEvent.type(inSheet.getByLabelText("CFOP dentro do estado"), "5102");
  await userEvent.type(inSheet.getByLabelText("CFOP interestadual"), "6102");
  await userEvent.click(inSheet.getByRole("button", { name: "Adicionar regra" }));
  await userEvent.type(inSheet.getByLabelText("Início do NCM da regra 1"), "1905");
  await userEvent.type(inSheet.getByLabelText("ICMS da regra 1"), "102");
  await userEvent.type(inSheet.getByLabelText("PIS da regra 1"), "07");
  await userEvent.type(inSheet.getByLabelText("Alíquota do PIS da regra 1"), "0,65");
  await userEvent.type(inSheet.getByLabelText("COFINS da regra 1"), "07");
  await userEvent.click(inSheet.getByRole("button", { name: "Adicionar natureza" }));

  // In the table, but not saved yet: nothing was sent.
  const row = (await screen.findByText("Venda de mercadoria")).closest("tr")!;
  expect(within(row).getByText("venda")).toBeInTheDocument();
  expect(within(row).getByText("5102 / 6102")).toBeInTheDocument();
  expect(screen.getByText("Há alterações ainda não salvas.")).toBeInTheDocument();
  expect(putsOf(fetchMock)).toEqual([]);

  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));
  await screen.findByText("Configurações fiscais salvas.");
  expect(putsOf(fetchMock)).toEqual([
    {
      ...stored,
      natures: [
        {
          code: "venda",
          name: "Venda de mercadoria",
          kind: "product",
          cfop_in_state: "5102",
          cfop_interstate: "6102",
          active: true,
          rules: [
            {
              regime: "simples_nacional",
              ncm_prefix: "1905",
              icms_code: "102",
              pis_code: "07",
              cofins_code: "07",
              icms_rate: null,
              pis_rate: "0.65",
              cofins_rate: null,
            },
          ],
          service: null,
        },
      ],
    },
  ]);
});

it("adds a service nature with no CFOP and no rules", async () => {
  const fetchMock = serve({ ...stored, natures: [] });
  renderAt("/companies/c1/fiscal/settings");
  await screen.findByDisplayValue("Verde Comércio Ltda");

  await userEvent.click(screen.getByRole("button", { name: "Adicionar natureza" }));
  const sheet = within(await screen.findByRole("dialog", { name: "Nova natureza" }));
  await userEvent.type(sheet.getByLabelText("Nome"), "Prestação de serviço");
  await userEvent.type(sheet.getByLabelText("Código"), "servico");
  await userEvent.selectOptions(sheet.getByLabelText("Tipo"), "service");
  expect(sheet.queryByLabelText("CFOP dentro do estado")).toBeNull();
  expect(sheet.queryByRole("button", { name: "Adicionar regra" })).toBeNull();
  await userEvent.click(sheet.getByRole("button", { name: "Adicionar natureza" }));
  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));

  await screen.findByText("Configurações fiscais salvas.");
  expect(putsOf(fetchMock)[0].natures).toEqual([
    { code: "servico", name: "Prestação de serviço", kind: "service", cfop_in_state: null, cfop_interstate: null, active: true, rules: [], service: null },
  ]);
});

it("sends the ISSQN terms of a service, and asks for the code and the taxation together", async () => {
  const fetchMock = serve({ ...stored, natures: [] });
  renderAt("/companies/c1/fiscal/settings");
  await screen.findByDisplayValue("Verde Comércio Ltda");

  await userEvent.click(screen.getByRole("button", { name: "Adicionar natureza" }));
  const sheet = within(await screen.findByRole("dialog", { name: "Nova natureza" }));
  await userEvent.type(sheet.getByLabelText("Nome"), "Prestação de serviço");
  await userEvent.type(sheet.getByLabelText("Código"), "servico");
  expect(sheet.queryByLabelText("Código nacional do serviço")).toBeNull();
  await userEvent.selectOptions(sheet.getByLabelText("Tipo"), "service");

  // Nothing typed: the terms are optional, and both fields stay free.
  expect(sheet.getByLabelText("Código nacional do serviço")).not.toBeRequired();
  expect(sheet.getByLabelText("Cobrança do ISS")).not.toBeRequired();
  // Once one is typed, the national code and the taxation are both needed.
  await userEvent.type(sheet.getByLabelText("Alíquota do ISS (%)"), "2,5");
  expect(sheet.getByLabelText("Código nacional do serviço")).toBeRequired();
  expect(sheet.getByLabelText("Cobrança do ISS")).toBeRequired();
  await userEvent.type(sheet.getByLabelText("Código nacional do serviço"), "010701");
  await userEvent.type(sheet.getByLabelText("Código municipal do serviço"), "1.07");
  await userEvent.selectOptions(sheet.getByLabelText("Cobrança do ISS"), "taxable");
  await userEvent.click(sheet.getByRole("button", { name: "Adicionar natureza" }));
  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));

  await screen.findByText("Configurações fiscais salvas.");
  expect(putsOf(fetchMock)[0].natures).toEqual([
    {
      code: "servico",
      name: "Prestação de serviço",
      kind: "service",
      cfop_in_state: null,
      cfop_interstate: null,
      active: true,
      rules: [],
      service: { national_tax_code: "010701", municipal_service_code: "1.07", iss_taxation: "taxable", iss_rate: "2.5" },
    },
  ]);
});

it("changes a nature that is in the table and takes one away", async () => {
  const fetchMock = serve(stored);
  renderAt("/companies/c1/fiscal/settings");
  await screen.findByText("Venda de mercadoria");

  await userEvent.click(screen.getByRole("button", { name: "Alterar Venda de mercadoria" }));
  const sheet = within(await screen.findByRole("dialog", { name: "Alterar Venda de mercadoria" }));
  // The code is the key a document keeps: it does not change.
  expect(sheet.getByLabelText("Código")).toHaveAttribute("readonly");
  await userEvent.clear(sheet.getByLabelText("Nome"));
  await userEvent.type(sheet.getByLabelText("Nome"), "Venda");
  await userEvent.click(sheet.getByRole("button", { name: "Concluir natureza" }));
  expect(await screen.findByText("Venda")).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "Remover Venda" }));
  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));
  await screen.findByText("Configurações fiscais salvas.");
  expect(putsOf(fetchMock)[0].natures).toEqual([]);
});

it.each([
  ["2027-03-31", "Vence em 136 dias (31/03/2027).", "status"],
  ["2026-11-20", "Certificado vence em 5 dias (20/11/2026). Renove-o no painel do gateway.", "status"],
  ["2026-11-15", "Certificado vence hoje (15/11/2026). Renove-o no painel do gateway.", "status"],
  ["2026-11-01", "Certificado vencido em 01/11/2026: nenhum documento pode ser emitido até renová-lo.", "alert"],
])("says where the certificate stands when it expires on %s", async (until, message, role) => {
  serve({ ...stored, certificate_valid_until: until });
  renderAt("/companies/c1/fiscal/settings");

  const text = await screen.findByText(message);
  if (role === "alert") expect(text.closest('[role="alert"]')).not.toBeNull();
});

it("computes the certificate's standing from the day it was typed", () => {
  expect(certificateStatus("2026-12-15", "2026-11-15")).toEqual({ kind: "expiring", daysLeft: 30 });
  expect(certificateStatus("2026-12-16", "2026-11-15")).toEqual({ kind: "valid", daysLeft: 31 });
  expect(certificateStatus("2026-11-14", "2026-11-15")).toEqual({ kind: "expired", daysAgo: 1 });
  expect(certificateStatus("", "2026-11-15")).toEqual({ kind: "unknown" });
});

it("lets a reader see the settings but not change them", async () => {
  serve(stored, withPermission("read"));
  renderAt("/companies/c1/fiscal/settings");

  const name = await screen.findByLabelText("Razão social");
  expect(name).toHaveValue("Verde Comércio Ltda");
  expect(name).toBeDisabled();
  expect(screen.getByLabelText("Regime tributário")).toBeDisabled();
  expect(screen.getByText("Venda de mercadoria")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Salvar" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Adicionar natureza" })).toBeNull();
  expect(screen.queryByRole("button", { name: /Alterar/ })).toBeNull();
});

it("shows what the server refuses, in its own words, and keeps what was typed", async () => {
  mockApi((url, init) => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/history")) return { status: 200, body: [] };
    if (init?.method === "PUT") return { status: 422, body: { error: "cnpj is not valid: only its shape is checked" } };
    return { status: 200, body: stored };
  });
  renderAt("/companies/c1/fiscal/settings");
  await screen.findByDisplayValue("Verde Comércio Ltda");

  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));

  expect(await screen.findByText(/cnpj is not valid/)).toBeInTheDocument();
  expect(screen.getByLabelText("Razão social")).toHaveValue("Verde Comércio Ltda");
});

it("says the series must be numbers before asking the server", async () => {
  const fetchMock = serve(stored);
  renderAt("/companies/c1/fiscal/settings");
  await screen.findByDisplayValue("Verde Comércio Ltda");

  await userEvent.clear(screen.getByLabelText("Série da NF-e"));
  await userEvent.type(screen.getByLabelText("Série da NF-e"), "a");
  await userEvent.click(screen.getByRole("button", { name: "Salvar" }));

  expect(await screen.findByText("Informe as séries como números, de 1 a 999.")).toBeInTheDocument();
  expect(putsOf(fetchMock)).toEqual([]);
});

it("shows the trail of changes", async () => {
  serve(stored);
  renderAt("/companies/c1/fiscal/settings");

  const change = (await screen.findByText("Regime tributário:")).closest("li")!;
  expect(change).toHaveTextContent("Regime tributário: Simples Nacional");
});

it("writes the natures out in the history so a changed rate or CFOP shows", async () => {
  mockApi((url) => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/history")) {
      return {
        status: 200,
        body: [
          {
            at: Date.UTC(2026, 10, 2, 15, 0),
            actor: { kind: "user", id: "u1", name: "Ana" },
            action: "updated",
            changes: [
              {
                field: "natures",
                from: "sale|Venda|product|5102|6102|true|simples_nacional::102:07:07:::",
                to: "sale|Venda|product|5102|6102|true|lucro_presumido:19:00:01:01:18:0.65:3",
              },
            ],
          },
        ],
      };
    }
    return { status: 200, body: stored };
  });
  renderAt("/companies/c1/fiscal/settings");

  expect(
    await screen.findByText(/Venda \(sale\), CFOP 5102\/6102, ativa \[Simples Nacional: ICMS 102, PIS 07, COFINS 07\]/),
  ).toBeInTheDocument();
  expect(
    screen.getByText(/Lucro Presumido NCM 19\*: ICMS 00 a 18%, PIS 01 a 0.65%, COFINS 01 a 3%/),
  ).toBeInTheDocument();
});

it.each([
  [null, /Nenhum gateway fiscal está configurado/],
  ["fake", /O gateway é o de teste/],
])("tells which gateway issues (%s)", async (gateway, message) => {
  mockApi((url) => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/fiscal/gateway")) return { status: 200, body: { gateway } };
    if (url.endsWith("/history")) return { status: 200, body: [] };
    return { status: 200, body: stored };
  });
  renderAt("/companies/c1/fiscal/settings");

  expect(await screen.findByText(message)).toBeInTheDocument();
});

it("says nothing of the gateway when it cannot be read or is a real one", async () => {
  mockApi((url) => {
    if (url === "/api/auth/me") return { status: 200, body: profile };
    if (url.endsWith("/fiscal/gateway")) return { status: 200, body: { gateway: "focus" } };
    if (url.endsWith("/history")) return { status: 200, body: [] };
    return { status: 200, body: stored };
  });
  renderAt("/companies/c1/fiscal/settings");

  await screen.findByLabelText("Razão social");
  expect(screen.queryByText(/gateway/i)).toBeNull();
});
