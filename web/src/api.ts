import { contentTypeOf } from "./lib/attachments";

export interface User {
  id: string;
  name: string;
  email: string;
}

export type Role = "owner" | "admin" | "financial" | "sales" | "stock" | "accountant" | "viewer";

/** A part of the product a permission is about (ADR 0026). */
export type Area = "parties" | "products" | "finance" | "sales" | "purchases" | "accounting" | "fiscal" | "members";

export interface Company {
  id: string;
  name: string;
  /** What the signed-in user is in this company. */
  role: Role;
  /** How far the role reaches into each area; an area it cannot reach is not there. The API says it, so the web app never knows the table. */
  permissions: Partial<Record<Area, "read" | "write">>;
}

/** Someone who belongs to a company, and what they are there. */
export interface Member {
  user_id: string;
  name: string;
  email: string;
  role: Role;
}

/** An offer to join a company that nobody has accepted or withdrawn. Times are milliseconds since the epoch. */
export interface Invitation {
  id: string;
  email: string;
  role: Role;
  created_at: number;
  expires_at: number;
  expired: boolean;
}

/** An invitation just made: the link that accepts it exists only in this answer. */
export interface Invited extends Invitation {
  link: string;
  /** Whether the e-mail with the link went out; the invitation exists either way. */
  emailed: boolean;
}

/** What a link leads to, shown before the person accepts. */
export interface InvitationPreview {
  company_name: string;
  email: string;
  role: Role;
  expires_at: number;
}

export interface Profile {
  user: User;
  companies: Company[];
  /** Whether the address was confirmed; the app asks for it until it is. */
  email_verified: boolean;
}

/** What Receita Federal said about a CNPJ when the party was registered. */
export interface DocumentCheck {
  status: "not_applicable" | "verified" | "not_found" | "unverified";
  legal_name: string | null;
  situation: string | null;
  /** The registry does not know the company, was unreachable, or the company is not active. Never blocks registering. */
  needs_attention: boolean;
}

/** A postal address: the CEP (digits only), street, city and state always, the rest when there is one. */
export interface Address {
  postal_code: string;
  street: string;
  number: string | null;
  complement: string | null;
  district: string | null;
  city: string;
  state: string;
}

/** A person to talk to at a party. */
export interface Contact {
  name: string;
  job_title: string | null;
  email: string | null;
  phone: string | null;
}

export interface Party {
  id: string;
  company_id: string;
  name: string;
  document: string | null;
  email: string | null;
  is_customer: boolean;
  is_supplier: boolean;
  is_carrier: boolean;
  is_seller: boolean;
  address: Address | null;
  /** In the order they were given. */
  contacts: Contact[];
  /** Lower case, each once, alphabetical. */
  tags: string[];
  document_check: DocumentCheck;
}

/** One recorded change to a party (ADR 0008). The API lists them oldest first. */
export interface HistoryEntry {
  /** Milliseconds since the Unix epoch. */
  at: number;
  actor: { kind: "user"; id: string; name: string };
  action: "created" | "updated";
  changes: { field: string; from: string | null; to: string | null }[];
}

/** What a postal code stands for, to fill an address in: the number and the complement stay the user's. */
export interface PostalCodeEntry {
  /** A city with one postal code for all of it has no street. */
  street: string | null;
  district: string | null;
  city: string;
  state: string;
}

/** A row of an import file with something to say about it; the number is the one a spreadsheet shows. */
export interface ImportNote {
  row: number;
  message: string;
}

/** What an import did, or with `dry_run` would do. Nothing is created while a row is refused. */
export interface ImportResult {
  dry_run: boolean;
  valid: number;
  created: number;
  /** Rows whose document, SKU or reference is already registered: never overwritten. */
  skipped: ImportNote[];
  refused: ImportNote[];
}

export interface PartyPage {
  items: Party[];
  next_cursor: string | null;
}

/** How a list is narrowed and ordered; the same three parameters every list endpoint takes. */
export interface PartyFilters {
  /** Text found in the name, e-mail or document. */
  q: string;
  kind: "" | "customer" | "supplier" | "carrier" | "seller";
  /** A tag the party carries, in any case. */
  tag: string;
  /** A leading `-` means descending. */
  sort: "name" | "-name";
}

export const NO_FILTERS: PartyFilters = { q: "", kind: "", tag: "", sort: "name" };

export interface AddressInput {
  postal_code: string;
  street: string;
  number?: string;
  complement?: string;
  district?: string;
  city: string;
  state: string;
}

export interface ContactInput {
  name: string;
  job_title?: string;
  email?: string;
  phone?: string;
}

/** The whole party, to register one or to replace one: what is left out is cleared. */
export interface PartyInput {
  name: string;
  document?: string;
  email?: string;
  is_customer: boolean;
  is_supplier: boolean;
  is_carrier: boolean;
  is_seller: boolean;
  address?: AddressInput;
  contacts: ContactInput[];
  tags: string[];
}

/** A sign-up either starts a company of its own, or joins the one an invitation's link leads to. */
export type Registration = { name: string; email: string; password: string } & (
  | { company_name: string }
  | { invitation: string }
);

export type ProductKind = "product" | "service";

export interface Product {
  id: string;
  company_id: string;
  kind: ProductKind;
  name: string;
  sku: string | null;
  unit: string;
  ncm: string | null;
  /** Decimal strings like `"12.5"`, never numbers: see `lib/decimal.ts`. */
  sale_price: string | null;
  cost_price: string | null;
  minimum_stock: string | null;
  active: boolean;
}

export interface ProductPage {
  items: Product[];
  next_cursor: string | null;
}

/** What is sent to register a product, and, whole, to change one: a change replaces every field. */
export interface ProductInput {
  kind: ProductKind;
  name: string;
  sku?: string;
  unit: string;
  ncm?: string;
  sale_price?: string;
  cost_price?: string;
  minimum_stock?: string;
  active: boolean;
}

export interface ProductFilters {
  q: string;
  kind: "" | ProductKind;
  active: "" | "true" | "false";
  sort: "name" | "-name";
}

export const NO_PRODUCT_FILTERS: ProductFilters = { q: "", kind: "", active: "", sort: "name" };

export interface CostCenter {
  id: string;
  company_id: string;
  name: string;
  active: boolean;
}

export interface CostCenterPage {
  items: CostCenter[];
  next_cursor: string | null;
}

export interface CostCenterInput {
  name: string;
  active: boolean;
}

export interface CostCenterFilters {
  q: string;
  active: "" | "true" | "false";
  sort: "name" | "-name";
}

export const NO_COST_CENTER_FILTERS: CostCenterFilters = { q: "", active: "", sort: "name" };

/** The keys the API stores and sends; `lib/groups.ts` has the labels people read. */
export type DreGroup =
  | "gross_revenue"
  | "deductions"
  | "costs"
  | "operating_expenses"
  | "financial_result"
  | "investments"
  | "outside_result"
  | "transfers";

export type CashFlowGroup = "operating" | "investing" | "financing" | "transfer";

/** A node of the tree of categories; `parent_id` says where it goes, `null` for a top-level one. */
export interface Category {
  id: string;
  company_id: string;
  parent_id: string | null;
  name: string;
  dre_group: DreGroup;
  cash_flow_group: CashFlowGroup;
  /** 1 for a top-level category. */
  level: number;
  active: boolean;
}

/** A top-level category chooses the groups; a subcategory names its parent and says nothing else. */
export interface NewCategory {
  name: string;
  parent_id?: string;
  dre_group?: DreGroup;
  cash_flow_group?: CashFlowGroup;
}

export type BankAccountKind = "checking" | "savings" | "cash" | "card";

export interface BankAccount {
  id: string;
  company_id: string;
  kind: BankAccountKind;
  name: string;
  /** 3 digits (COMPE), or 8 (ISPB) for an institution without one. */
  bank_code: string | null;
  /** Upper case letters and digits, without the punctuation it was written with. */
  branch: string | null;
  number: string | null;
  active: boolean;
}

export interface BankAccountPage {
  items: BankAccount[];
  next_cursor: string | null;
}

/** What is sent to register an account, and, whole, to change one: a change replaces every field. */
export interface BankAccountInput {
  kind: BankAccountKind;
  name: string;
  bank_code?: string;
  branch?: string;
  number?: string;
  active: boolean;
}

export interface BankAccountFilters {
  q: string;
  kind: "" | BankAccountKind;
  active: "" | "true" | "false";
  sort: "name" | "-name";
}

export const NO_BANK_ACCOUNT_FILTERS: BankAccountFilters = { q: "", kind: "", active: "", sort: "name" };

export type PaymentMethodKind =
  | "cash"
  | "pix"
  | "bank_slip"
  | "credit_card"
  | "debit_card"
  | "bank_transfer"
  | "check"
  | "other";

export interface PaymentMethod {
  id: string;
  company_id: string;
  kind: PaymentMethodKind;
  name: string;
  active: boolean;
}

export interface PaymentMethodPage {
  items: PaymentMethod[];
  next_cursor: string | null;
}

export interface PaymentMethodInput {
  kind: PaymentMethodKind;
  name: string;
  active: boolean;
}

export interface PaymentMethodFilters {
  q: string;
  kind: "" | PaymentMethodKind;
  active: "" | "true" | "false";
  sort: "name" | "-name";
}

export const NO_PAYMENT_METHOD_FILTERS: PaymentMethodFilters = { q: "", kind: "", active: "", sort: "name" };

/** Due `days` after the date the term is applied to, for `percent` of the amount (a decimal string). */
export interface Installment {
  days: number;
  percent: string;
}

export interface PaymentTerm {
  id: string;
  company_id: string;
  name: string;
  /** In order of due date: the days increase, and the percentages add up to exactly 100. */
  installments: Installment[];
  active: boolean;
}

export interface PaymentTermPage {
  items: PaymentTerm[];
  next_cursor: string | null;
}

export interface PaymentTermInput {
  name: string;
  installments: Installment[];
  active: boolean;
}

export interface PaymentTermFilters {
  q: string;
  active: "" | "true" | "false";
  sort: "name" | "-name";
}

export const NO_PAYMENT_TERM_FILTERS: PaymentTermFilters = { q: "", active: "", sort: "name" };

/** What each product sells for in a set of unit prices (ADR 0042); choosing a list only fills the price of a line. */
export interface PriceList {
  id: string;
  company_id: string;
  name: string;
  /** In the order they were written, each product once. */
  items: { product_id: string; price: string }[];
  active: boolean;
}

export interface PriceListPage {
  items: PriceList[];
  next_cursor: string | null;
}

/** Replaces the whole list. */
export interface PriceListInput {
  name: string;
  items: { product_id: string; price: string }[];
  active: boolean;
}

export interface PriceListFilters {
  q: string;
  active: "" | "true" | "false";
  sort: "name" | "-name";
}

export const NO_PRICE_LIST_FILTERS: PriceListFilters = { q: "", active: "", sort: "name" };

export type SalesGroup = "customer" | "product" | "seller";

export interface SalesReportRow {
  /** The customer, product or seller; null for the sales that name no seller. */
  key: string | null;
  /** How many completed sales it is in. */
  sales: number;
  /** The value sold net of returns, a decimal string. */
  net: string;
  /** Its part of the total in percent, at most two decimals. */
  share: string;
  /** The share of this row and the ones before it. */
  cumulative: string;
  /** Where the row starts on the ABC curve: A up to 80% of the total, B up to 95%, C after. */
  class: "A" | "B" | "C";
}

/** What the completed sales of a span add up to, biggest first (ADR 0043). */
export interface SalesReport {
  from: string;
  to: string;
  by: SalesGroup;
  sales: number;
  total: string;
  rows: SalesReportRow[];
}

/** What each group of the managerial income statement added up to, and the subtotals the server worked out (ADR 0070). */
export interface DreLines {
  gross_revenue: string;
  deductions: string;
  costs: string;
  operating_expenses: string;
  financial_result: string;
  investments: string;
  outside_result: string;
  net_revenue: string;
  gross_profit: string;
  operating_result: string;
  net_result: string;
}

export type DreBy = "month" | "cost_center";

export interface DreColumn {
  /** `YYYY-MM` for a month; a cost center's id, or null for the entries that have none. */
  key: string | null;
  lines: DreLines;
}

export interface Dre {
  from: string;
  to: string;
  by: DreBy;
  columns: DreColumn[];
  total: DreLines;
}

/** A payment of an installment, in part or in full (ADR 0030). */
export interface Settlement {
  id: string;
  entry_id: string;
  installment: number;
  settled_on: string;
  /** How much of the installment it settles. */
  applied: string;
  interest: string;
  fine: string;
  discount: string;
  /** What moved in the account: applied, with interest and fine, less the discount. */
  cash: string;
  bank_account_id: string;
  payment_method_id: string | null;
  reversed: boolean;
}

export interface SettlementInput {
  installment: number;
  settled_on: string;
  applied: string;
  interest?: string;
  fine?: string;
  discount?: string;
  bank_account_id: string;
  payment_method_id?: string;
}

/** A file attached to an entry (ADR 0036). */
export interface Attachment {
  id: string;
  entry_id: string;
  file_name: string;
  content_type: string;
  /** In bytes. */
  size: number;
}

/** How a customer is asked to pay (ADR 0045). */
export type ChargeMethod = "boleto" | "pix" | "card_link";

export type ChargeStatus = "open" | "paid" | "cancelled" | "expired";

/** A request to be paid for one installment of a receivable, made at a payment provider (ADR 0045). */
export interface Charge {
  id: string;
  entry_id: string;
  installment: number;
  method: ChargeMethod;
  status: ChargeStatus;
  /** What was left of the installment when it was made, a decimal string. */
  amount: string;
  /** `YYYY-MM-DD`. */
  due_on: string;
  provider: string;
  /** The boleto's line, to type in an app. */
  digitable_line: string | null;
  /** The Pix copy-and-paste text. */
  pix_payload: string | null;
  /** The provider's page: the boleto's paper or the card checkout. */
  payment_url: string | null;
  issued_by: string;
  /** What the provider said was paid, once it was (ADR 0046). */
  paid_on: string | null;
  paid_amount: string | null;
  fee: string | null;
  /** Why the installment was not settled by the payment, when it was not: someone has to look at it. */
  note: string | null;
}

/** What the customer sees at the link of a charge (ADR 0047): nothing of the company's books. */
export interface PublicCharge {
  company_name: string;
  description: string;
  installment: number;
  installments: number;
  method: ChargeMethod;
  /** What the page may offer: pay, say it was paid, or say there is nothing to pay. */
  state: "payable" | "paid" | "unavailable";
  amount: string;
  due_on: string;
  digitable_line: string | null;
  pix_payload: string | null;
  payment_url: string | null;
  paid_on: string | null;
}

/** What the accountant can download: one file per table, or all of them with the attachments in a zip. */
export type ExportFile = "entries" | "settlements" | "transfers" | "package";
export type ExportFormat = "csv" | "xlsx";

/** The company's closed period (ADR 0069): everything dated on or before the day is closed. */
export interface PeriodLock {
  locked_through: string | null;
}

/** When the accountants are told the previous month's package is ready (ADR 0072). */
export interface PackageSchedule {
  enabled: boolean;
  /** The day of the month, 1 to 28, from which it is announced. */
  day: number;
  /** How many members it would reach: the ones whose role is accountant. With none, nothing is sent. */
  accountants: number;
}

export type TaxRegime = "simples_nacional" | "lucro_presumido";
export type FiscalEnvironment = "homologation" | "production";
export type NatureKind = "product" | "service";

/** The codes and rates a company puts on the items of a nature, for one regime and, optionally, an NCM prefix (ADR 0073). */
export interface TaxRule {
  regime: TaxRegime;
  ncm_prefix: string | null;
  /** A CSOSN (3 digits) for the Simples Nacional, a CST (2 digits) otherwise. */
  icms_code: string;
  pis_code: string;
  cofins_code: string;
  /** Percentages as decimal text, never a number. */
  icms_rate: string | null;
  pis_rate: string | null;
  cofins_rate: string | null;
}

/** What a company calls an operation, with the CFOPs of its goods and its tax rules; a service has neither. */
export interface OperationNature {
  code: string;
  name: string;
  kind: NatureKind;
  cfop_in_state: string | null;
  cfop_interstate: string | null;
  active: boolean;
  rules: TaxRule[];
}

export type FiscalDocumentStatus =
  | "draft"
  | "processing"
  | "authorized"
  | "contingency"
  | "rejected"
  | "denied"
  | "cancelled";

export interface FiscalRefusal {
  code: string;
  message: string;
}

/** An NF-e on its way to, or past, the authority (ADR 0076). */
export interface FiscalDocument {
  id: string;
  kind: "nfe";
  status: FiscalDocumentStatus;
  /** The cancellation waiting for the authority's answer, or accepted (a gateway may answer it later, ADR 0078). */
  cancellation: FiscalEvent | null;
  sale_id: string;
  issuer_cnpj: string;
  issuer_name: string;
  environment: FiscalEnvironment;
  recipient_name: string;
  recipient_document: string | null;
  nature_code: string;
  total: string;
  issued_on: string;
  access_key: string | null;
  number: number | null;
  series: number | null;
  protocol: string | null;
  refusal: FiscalRefusal | null;
  note: string | null;
  submitted_by: string;
  /** The files that can be downloaded: `xml`, `pdf`, `cancellation_xml`. */
  files: FiscalFileKind[];
}

export type FiscalFileKind = "xml" | "pdf" | "cancellation_xml";

export interface FiscalDocumentPage {
  items: FiscalDocument[];
  next_cursor: string | null;
}

export type BuyerPresence =
  | "not_applicable"
  | "in_person"
  | "internet"
  | "telephone"
  | "home_delivery"
  | "outside_the_establishment"
  | "other";

export type PaymentForm =
  | "cash"
  | "cheque"
  | "credit_card"
  | "debit_card"
  | "store_credit"
  | "bank_slip"
  | "bank_deposit"
  | "pix"
  | "transfer"
  | "no_payment"
  | "other";

/** Every tax choice is the person's: nothing has a default (ADR 0076). */
export interface IssueChoices {
  sale_id: string;
  nature_code: string;
  origin: number;
  presence: BuyerPresence;
  payment: PaymentForm;
  final_consumer: boolean;
}

export type FiscalEventStatus = "sent" | "accepted" | "refused";

/** A correction letter or an invalidation of numbers (ADR 0076). */
export interface FiscalEvent {
  id: string;
  kind: "correction" | "cancellation" | "invalidation";
  status: FiscalEventStatus;
  text: string;
  sequence: number | null;
  series: number | null;
  first: number | null;
  last: number | null;
  protocol: string | null;
  refusal: FiscalRefusal | null;
  requested_by: string;
  has_file: boolean;
}

export interface FiscalDocumentFilters {
  status: FiscalDocumentStatus | "";
}

/** How the company issues fiscal documents (ADR 0073): the whole settings are replaced by one `PUT`. */
export interface FiscalSettings {
  legal_name: string;
  cnpj: string;
  state_registration: string | null;
  municipal_registration: string | null;
  address: Address;
  regime: TaxRegime;
  environment: FiscalEnvironment;
  nfe_series: number;
  nfce_series: number;
  nfse_series: number;
  /** The day the A1 certificate expires, as typed: the certificate itself stays at the gateway. */
  certificate_valid_until: string | null;
  natures: OperationNature[];
}

/** Where the money of a paid charge lands, and what the provider's fee is entered under (ADR 0046). */
export interface CollectionSettings {
  bank_account_id: string | null;
  fee_category_id: string | null;
}

/** One range of lateness of the delinquency report (ADR 0051). */
export interface DelinquencyBucket {
  from_days: number;
  /** `null` for the last range, which has no end. */
  to_days: number | null;
  owed: string;
  installments: number;
}

export interface DelinquentCustomer {
  /** `null` for what has no customer. */
  party_id: string | null;
  owed: string;
  installments: number;
  /** `YYYY-MM-DD`. */
  oldest_due: string;
  /** What is owed in each range, in the order of `buckets`. */
  by_bucket: string[];
}

/** Who owes what late on a day, and how late (ADR 0051). */
export interface Delinquency {
  on: string;
  owed: string;
  installments: number;
  buckets: DelinquencyBucket[];
  customers: DelinquentCustomer[];
}

export type DunningChannel = "email" | "whatsapp";

/** One reminder: on the due date moved by `days` (negative before it, 0 on it, positive after it) (ADR 0048). */
export interface DunningRule {
  days: number;
  channel: DunningChannel;
}

/** When customers are reminded of what they owe, a plan per company (ADR 0048). */
export interface DunningPlan {
  enabled: boolean;
  rules: DunningRule[];
  /** Whether the operator configured a WhatsApp provider: without one a rule for it is never tried. */
  whatsapp_available: boolean;
}

/** A reminder sent (or tried) to a customer about an installment. */
export interface Reminder {
  installment: number;
  days: number;
  channel: DunningChannel;
  recipient: string;
  status: "pending" | "sent" | "failed";
  attempts: number;
  note: string | null;
}

export interface ChargeInput {
  installment: number;
  method: ChargeMethod;
  /** `YYYY-MM-DD`, when it is not the installment's own. */
  due_on?: string;
}

/** Money moved from one account of the company to another (ADR 0031). */
export interface Transfer {
  id: string;
  company_id: string;
  from_account_id: string;
  to_account_id: string;
  /** A decimal string without trailing zeros, in cents at most. */
  amount: string;
  /** `YYYY-MM-DD`. */
  occurred_on: string;
  note: string | null;
  /** Undone: it stays listed and counts in no balance. */
  reversed: boolean;
}

export interface TransferPage {
  items: Transfer[];
  next_cursor: string | null;
}

export interface TransferInput {
  from_account_id: string;
  to_account_id: string;
  amount: string;
  occurred_on: string;
  note?: string;
}

export interface TransferFilters {
  /** The id of an account: the transfers that come from it or go to it. Empty for all of them. */
  account: string;
}

export const NO_TRANSFER_FILTERS: TransferFilters = { account: "" };

/** What an account held at the start of a day. */
export interface OpeningBalance {
  /** A signed decimal string: an account in overdraft starts below zero. */
  balance: string;
  on: string;
}

/** How an account got to its balance on a day (ADR 0031); `opening` is null until one is set. */
export interface AccountBalance {
  account: BankAccount;
  opening: OpeningBalance | null;
  received: string;
  paid: string;
  transfers_in: string;
  transfers_out: string;
  balance: string;
}

export interface Balances {
  on: string;
  accounts: AccountBalance[];
  /** The sum of every account's balance, closed ones included. */
  total: string;
}

/** What a cash flow is asked for (ADR 0032): a span of days cut into periods, over every account or one. */
export interface CashFlowFilters {
  /** `YYYY-MM-DD`. */
  from: string;
  to: string;
  granularity: "day" | "month";
  /** The id of one account, or empty for all of them. */
  account: string;
}

/** What happened in one period, and what is still to happen in it. Amounts are decimal strings. */
export interface CashFlowPeriod {
  start: string;
  end: string;
  received: string;
  paid: string;
  transfers_in: string;
  transfers_out: string;
  net: string;
  /** The opening balances whose day falls in the period. */
  new_opening_balances: string;
  closing_balance: string;
  /** Open on the installments due in the period; null when one account is asked for. */
  to_receive: string | null;
  to_pay: string | null;
  projected_balance: string | null;
}

export interface CashFlow {
  from: string;
  to: string;
  granularity: "day" | "month";
  account_id: string | null;
  /** The balance at the start of `from`. */
  opening_balance: string;
  /** Still open on installments that fell due before `from`, counted by the projected balances; null for one account. */
  past_due: { to_receive: string; to_pay: string } | null;
  periods: CashFlowPeriod[];
}

export type EntryKind = "receivable" | "payable";

export type ReportGroup = "category" | "cost_center" | "party";
export type ReportBasis = "competence" | "due";

/** One group of the entries report: what was entered as receivable and payable, and in how many entries. */
export interface ReportRow {
  /** The category, cost center or party; null for what has no cost center or no party. */
  id: string | null;
  receivable: string;
  payable: string;
  /** Receivable less payable. */
  net: string;
  entries: number;
}

export interface EntriesReport {
  group: ReportGroup;
  basis: ReportBasis;
  from: string;
  to: string;
  /** The biggest first. */
  rows: ReportRow[];
}

export interface EntriesReportFilters {
  group: ReportGroup;
  basis: ReportBasis;
  from: string;
  to: string;
}

/** An installment that fell due and still has something left (ADR 0035). */
export interface OverdueItem {
  entry_id: string;
  kind: EntryKind;
  description: string;
  party_id: string | null;
  category_id: string;
  installment: number;
  due_date: string;
  amount: string;
  settled: string;
  open: string;
  days_late: number;
}

export interface OverdueReport {
  on: string;
  /** Oldest due day first, cut at the limit. */
  items: OverdueItem[];
  /** What is open on all of them, and how many there are, whatever the limit. */
  receivable: string;
  payable: string;
  count: number;
}

export type StatementLineStatus = "pending" | "matched" | "ignored";

/** One line of a bank statement and where it stands in the reconciliation (ADR 0034). */
export interface StatementLine {
  id: string;
  company_id: string;
  account_id: string;
  /** `YYYY-MM-DD`. */
  posted_on: string;
  /** Signed: positive came into the account, negative went out of it. */
  amount: string;
  description: string;
  bank_ref: string | null;
  status: StatementLineStatus;
  matched_settlement_id: string | null;
  matched_transfer_id: string | null;
}

export interface StatementLinePage {
  items: StatementLine[];
  next_cursor: string | null;
}

export interface StatementLineFilters {
  /** The id of an account, or empty for all of them. */
  account: string;
  status: "" | StatementLineStatus;
}

export const NO_STATEMENT_LINE_FILTERS: StatementLineFilters = { account: "", status: "pending" };

/** What a pending line may be matched to: a settlement of an entry, or a transfer. */
export type LineSuggestion =
  | { kind: "settlement"; settlement: Settlement; entry_description: string }
  | { kind: "transfer"; transfer: Transfer };

export interface EntryFromLineInput {
  category_id: string;
  party_id?: string;
  cost_center_id?: string;
  payment_method_id?: string;
  description?: string;
}

export type RecurrenceFrequency = "weekly" | "monthly" | "yearly";

/** An entry that comes back every week, month or year and is generated ahead (ADR 0033). */
export interface Recurrence {
  id: string;
  company_id: string;
  kind: EntryKind;
  description: string;
  party_id: string | null;
  category_id: string;
  cost_center_id: string | null;
  /** A decimal string in cents: what every occurrence is worth. */
  amount: string;
  frequency: RecurrenceFrequency;
  /** `YYYY-MM-DD`: the day of the first occurrence. */
  starts_on: string;
  ends_on: string | null;
  active: boolean;
  /** The day through which its entries were made. */
  generated_through: string | null;
}

export interface RecurrencePage {
  items: Recurrence[];
  next_cursor: string | null;
}

/** Replaces the whole recurrence: a reference left out is removed. What was generated is not touched. */
export interface RecurrenceInput {
  kind: EntryKind;
  description: string;
  party_id?: string;
  category_id: string;
  cost_center_id?: string;
  amount: string;
  frequency: RecurrenceFrequency;
  starts_on: string;
  ends_on?: string;
  active: boolean;
}

export interface RecurrenceFilters {
  q: string;
  kind: "" | EntryKind;
  active: "" | "true" | "false";
  sort: "description" | "-description";
}

export const NO_RECURRENCE_FILTERS: RecurrenceFilters = { q: "", kind: "", active: "", sort: "description" };

export type QuoteStatus = "draft" | "sent" | "approved" | "declined" | "converted";

/** One line of a quote, a sale or a contract; every figure is a decimal string (ADR 0037). */
export interface DocumentLine {
  product_id: string;
  description: string;
  quantity: string;
  unit_price: string;
  discount: string;
  /** The rounded gross less the discount. */
  total: string;
}

/** What the company offers a customer (ADR 0038). */
export interface Quote {
  id: string;
  company_id: string;
  /** The company's own sequence. */
  number: number;
  customer_id: string;
  issued_on: string;
  valid_until: string;
  payment_term_id: string | null;
  notes: string | null;
  status: QuoteStatus;
  lines: DocumentLine[];
  total: string;
}

export interface QuotePage {
  items: Quote[];
  next_cursor: string | null;
}

export interface LineInput {
  product_id: string;
  description: string;
  quantity: string;
  unit_price: string;
  discount?: string;
}

/** Replaces the whole quote while it is a draft or sent. */
export interface QuoteInput {
  customer_id: string;
  issued_on: string;
  valid_until: string;
  payment_term_id?: string;
  notes?: string;
  lines: LineInput[];
}

export interface QuoteFilters {
  q: string;
  status: "" | QuoteStatus;
  sort: "-number" | "number";
}

export const NO_QUOTE_FILTERS: QuoteFilters = { q: "", status: "", sort: "-number" };

/** A quote just sent: the link the customer answers through exists only in this answer. */
export interface SentQuote {
  quote: Quote;
  link: string;
  /** Whether the e-mail went out; the quote is sent either way. */
  emailed: boolean;
}

/** What the customer sees at the link. */
export interface PublicQuote {
  company_name: string;
  customer_name: string;
  number: number;
  issued_on: string;
  valid_until: string;
  /** The validity has run out today: it can no longer be approved. */
  expired: boolean;
  payment_term: string | null;
  notes: string | null;
  status: QuoteStatus;
  /** Without the product: the customer is shown no id of the company's books. */
  lines: Omit<DocumentLine, "product_id">[];
  total: string;
}

export type SaleStatus = "open" | "completed" | "cancelled";

/** An order that becomes a sale (ADR 0039). */
export interface Sale {
  id: string;
  company_id: string;
  number: number;
  customer_id: string;
  seller_id: string | null;
  sold_on: string;
  payment_term_id: string | null;
  /** The category of the receivable the sale makes. */
  category_id: string;
  cost_center_id: string | null;
  notes: string | null;
  status: SaleStatus;
  quote_id: string | null;
  /** The contract it bills a cycle of, and the day of the cycle (ADR 0040). */
  contract_id: string | null;
  cycle_on: string | null;
  lines: DocumentLine[];
  total: string;
  /** Only the answers about one sale say it; the lists leave it out. */
  receivable_id: string | null;
}

export interface SalePage {
  items: Sale[];
  next_cursor: string | null;
}

/** Replaces the whole order while it is open. */
export interface SaleInput {
  customer_id: string;
  seller_id?: string;
  sold_on: string;
  payment_term_id?: string;
  category_id: string;
  cost_center_id?: string;
  notes?: string;
  lines: LineInput[];
}

export interface SaleFilters {
  q: string;
  status: "" | SaleStatus;
  sort: "-number" | "number";
}

export const NO_SALE_FILTERS: SaleFilters = { q: "", status: "", sort: "-number" };

/** What a quote becomes a sale with: what the quote does not know. */
export interface ConversionInput {
  sold_on: string;
  category_id: string;
  cost_center_id?: string;
  seller_id?: string;
  payment_term_id?: string;
}

export interface ReturnInput {
  returned_on: string;
  reason?: string;
  /** `line` is the line's place in the sale, from 1. */
  items: { line: number; quantity: string }[];
  /** The category of the payable that gives the money back. */
  refund_category_id: string;
  refund_due_on: string;
}

export interface SaleReturn {
  id: string;
  sale_id: string;
  returned_on: string;
  reason: string | null;
  items: { line: number; quantity: string }[];
  refund: string;
  /** `null` only if the payable's write failed after the return was stored. */
  refund_entry_id: string | null;
}

export type PurchaseStatus = "draft" | "approved" | "partially_received" | "received" | "cancelled";

/** An order to a supplier (ADR 0053); a line's `unit_price` is the unit cost. */
export interface PurchaseOrder {
  id: string;
  company_id: string;
  number: number;
  supplier_id: string;
  ordered_on: string;
  expected_on: string | null;
  payment_term_id: string | null;
  /** The category of the payables the receipts make. */
  category_id: string;
  cost_center_id: string | null;
  notes: string | null;
  status: PurchaseStatus;
  /** The access key of the supplier's NF-e it was entered from; `null` for an order made by hand. */
  invoice_key: string | null;
  lines: DocumentLine[];
  total: string;
}

export interface PurchaseOrderPage {
  items: PurchaseOrder[];
  next_cursor: string | null;
}

/** Replaces the whole order while it is a draft. */
export interface PurchaseOrderInput {
  supplier_id: string;
  ordered_on: string;
  expected_on?: string;
  payment_term_id?: string;
  category_id: string;
  cost_center_id?: string;
  notes?: string;
  lines: LineInput[];
}

/** What asking for a document by e-mail answers: where it went, and whether it was taken for sending. */
export interface Mailed {
  to: string;
  emailed: boolean;
}

/** That a document was e-mailed: to whom, and when (Unix milliseconds). */
export interface SentMail {
  to: string;
  sent_at_millis: number;
}

export interface PurchaseOrderFilters {
  q: string;
  status: "" | PurchaseStatus;
  /** `YYYY-MM-DD`: the orders made from this day to that day (empty is no limit). */
  from: string;
  to: string;
  sort: "-number" | "number";
}

export const NO_PURCHASE_FILTERS: PurchaseOrderFilters = { q: "", status: "", from: "", to: "", sort: "-number" };

/** How many orders match the search in each status, for the tiles of the list. */
export type PurchaseStatusCounts = Record<PurchaseStatus, number>;

export interface ReceiptItem {
  /** The line's place in the order, from 1. */
  line: number;
  quantity: string;
  /** What the quantity cost. */
  value: string;
}

export interface PurchaseReceipt {
  id: string;
  purchase_id: string;
  received_on: string;
  notes: string | null;
  items: ReceiptItem[];
  /** The sum of the items, what the payable pays. */
  total: string;
  /** `null` for a receipt worth nothing, or while its payable is still to be made (finishing the receipt makes it). */
  payable_id: string | null;
}

export interface ReceiptInput {
  received_on: string;
  notes?: string;
  items: { line: number; quantity: string }[];
}

export type StockMovementKind = "in" | "out" | "adjustment";
export type StockOriginKind = "purchase_receipt" | "sale" | "sale_return" | "inventory_count";

/** What a product has on hand, on a day or now (ADR 0054). */
export interface StockPosition {
  product_id: string;
  name: string;
  sku: string | null;
  unit: string;
  active: boolean;
  minimum_stock: string | null;
  /** Decimal strings: the quantity with up to 3 decimals, the value in reais. */
  quantity: string;
  value: string;
  /** The value over the quantity with six decimals; `null` without something on hand. */
  average_cost: string | null;
}

export interface StockPositionPage {
  items: StockPosition[];
  next_cursor: string | null;
}

export type StockSituation = "in_stock" | "below_minimum" | "depleted";

export interface StockPositionFilters {
  q: string;
  /** Only the products in this situation; empty is every product. */
  situation: "" | StockSituation;
  /** `YYYY-MM-DD`: the movements through this day; empty is now. */
  on: string;
  sort: "name" | "-name";
}

export const NO_POSITION_FILTERS: StockPositionFilters = { q: "", situation: "", on: "", sort: "name" };

/** The products that match the search, counted by situation, and what they hold (decimal string, in reais). */
export interface StockPositionSummary {
  total: number;
  in_stock: number;
  below_minimum: number;
  depleted: number;
  value: string;
}

export interface StockMovement {
  id: string;
  product_id: string;
  kind: StockMovementKind;
  /** Signed: what comes in is above 0, what goes out below. */
  quantity: string;
  /** Signed like the quantity. */
  value: string;
  occurred_on: string;
  /** The document that made it; `null` for one written by hand. */
  origin: { kind: StockOriginKind; id: string; line: number } | null;
  /** The movement this one undoes. */
  reverses: string | null;
  note: string | null;
}

export interface StockMovementPage {
  items: StockMovement[];
  next_cursor: string | null;
}

export interface StockMovementFilters {
  product: string;
  kind: "" | StockMovementKind;
  from: string;
  to: string;
}

export const NO_MOVEMENT_FILTERS: StockMovementFilters = { product: "", kind: "", from: "", to: "" };

/** How many movements match the product and days in each kind, for the tiles of the ledger. */
export type StockKindCounts = Record<StockMovementKind, number>;

/** A movement written by hand; an `in` takes its cost as `value` (the total) or `unit_cost`, not both. */
export interface StockMovementInput {
  product_id: string;
  kind: StockMovementKind;
  /** Above 0 for an `in` and an `out`; signed for an `adjustment`. */
  quantity: string;
  value?: string;
  unit_cost?: string;
  occurred_on: string;
  note?: string;
}

export interface StockReverseInput {
  occurred_on: string;
  note?: string;
}

/** An active product with less than its minimum on hand now (ADR 0055); every figure a decimal string. */
export interface StockAlert {
  product_id: string;
  name: string;
  sku: string | null;
  unit: string;
  quantity: string;
  minimum_stock: string;
  /** What is missing to reach the minimum. */
  shortfall: string;
}

export interface StockSettings {
  /** Whether completing a sale that the stock cannot cover is refused. */
  block_sale_without_balance: boolean;
}

export type InventoryCountStatus = "open" | "applied" | "cancelled";

export interface InventoryCountLine {
  product_id: string;
  /** What was found: a decimal string. */
  counted: string;
  /** What the stock held (now while open, at the application once applied); `null` in a list and when cancelled. */
  expected: string | null;
  /** Counted less expected, signed; `null` where `expected` is. */
  difference: string | null;
}

/** A count of what is on the shelf, which an application turns into stock adjustments (ADR 0056). */
export interface InventoryCount {
  id: string;
  company_id: string;
  number: number;
  counted_on: string;
  notes: string | null;
  status: InventoryCountStatus;
  lines: InventoryCountLine[];
}

export interface InventoryCountPage {
  items: InventoryCount[];
  next_cursor: string | null;
}

/** Replaces the whole count while it is open. */
export interface InventoryCountInput {
  counted_on: string;
  notes?: string;
  lines: { product_id: string; counted: string }[];
}

export interface InventoryCountFilters {
  q: string;
  status: "" | InventoryCountStatus;
  sort: "-number" | "number";
}

export const NO_COUNT_FILTERS: InventoryCountFilters = { q: "", status: "", sort: "-number" };

/** One item of a supplier's NF-e as the preview reads it (ADR 0057); every figure is a decimal string. */
export interface InvoiceItem {
  number: number;
  /** The supplier's code for the product. */
  code: string;
  ean: string | null;
  description: string;
  ncm: string | null;
  unit: string;
  quantity: string;
  unit_value: string;
  product_value: string;
  discount: string;
  /** What the item costs: its products less discount plus the charges on it. */
  cost: string;
  ipi: string;
  icms_st: string;
  icms: string;
  /** The product registered with this code as its SKU, if one is. */
  product_id: string | null;
}

export interface InvoicePreview {
  access_key: string;
  number: string;
  series: string;
  issued_on: string;
  issuer: { document: string; name: string };
  total: string;
  /** The supplier registered with the issuer's document; `null` when there is none. */
  supplier_id: string | null;
  items: InvoiceItem[];
  /** What the invoice says is paid; empty when it does not say. */
  installments: { due_date: string; amount: string }[];
  /** The purchase the invoice was already entered as. */
  entered: { purchase_id: string; number: number; status: PurchaseStatus } | null;
}

/** What the person decides when entering an invoice; the file is sent as the body. */
export interface InvoiceEntryParams {
  category_id: string;
  cost_center_id?: string;
  payment_term_id?: string;
  received_on?: string;
  supplier_id?: string;
  /** The product of each item in the order of the items. */
  products: string[];
}

export interface InvoiceEntry {
  purchase_id: string;
  number: number;
  status: PurchaseStatus;
  receipt: PurchaseReceipt;
  /** The invoice had been entered before: nothing was made twice. */
  already_entered: boolean;
}

export interface StockValuationRow {
  product_id: string;
  name: string;
  sku: string | null;
  unit: string;
  quantity: string;
  /** The value over the quantity with six decimals; `null` without something on hand. */
  average_cost: string | null;
  value: string;
  /** Its part of the total value, in percent with at most two decimals. */
  share: string;
}

/** What the stock was worth on a day, by product and in all (ADR 0058). */
export interface StockValuation {
  on: string;
  total: string;
  /** How many products had less than nothing on hand. */
  negative: number;
  /** Biggest value first. */
  rows: StockValuationRow[];
}

export interface StockMovementSummaryRow {
  product_id: string;
  name: string;
  sku: string | null;
  unit: string;
  opening_quantity: string;
  opening_value: string;
  entered_quantity: string;
  entered_value: string;
  /** What went out, as positive numbers. */
  exited_quantity: string;
  exited_value: string;
  /** What counts and adjustments added; negative when they took away. */
  adjusted_quantity: string;
  adjusted_value: string;
  closing_quantity: string;
  closing_value: string;
}

export interface StockMovementSummary {
  from: string;
  to: string;
  items: StockMovementSummaryRow[];
}

export type BillingCycle = "monthly" | "bimonthly" | "quarterly" | "semiannual" | "yearly";
export type ContractStatus = "active" | "paused" | "terminated";

/** An agreement that bills the same lines every cycle, each as a completed sale (ADR 0040). */
export interface Contract {
  id: string;
  company_id: string;
  name: string;
  customer_id: string;
  seller_id: string | null;
  payment_term_id: string | null;
  category_id: string;
  cost_center_id: string | null;
  notes: string | null;
  lines: DocumentLine[];
  /** What one cycle bills: the sum of the lines. */
  total: string;
  cycle: BillingCycle;
  starts_on: string;
  ends_on: string | null;
  status: ContractStatus;
  /** The day through which the cycles were billed. */
  billed_through: string | null;
}

export interface ContractPage {
  items: Contract[];
  next_cursor: string | null;
}

/** Replaces the whole contract; what was billed is not touched. */
export interface ContractInput {
  name: string;
  customer_id: string;
  seller_id?: string;
  payment_term_id?: string;
  category_id: string;
  cost_center_id?: string;
  notes?: string;
  lines: LineInput[];
  cycle: BillingCycle;
  starts_on: string;
  ends_on?: string;
  active: boolean;
}

export interface ContractFilters {
  q: string;
  status: "" | ContractStatus;
  sort: "name" | "-name";
}

export const NO_CONTRACT_FILTERS: ContractFilters = { q: "", status: "", sort: "name" };

/** What a seller earns on completed sales: the most specific rule in force on the sale's day wins (ADR 0041). */
export interface CommissionRule {
  id: string;
  company_id: string;
  seller_id: string | null;
  product_id: string | null;
  /** A percentage above 0 and at most 100, with at most two decimals. */
  rate: string;
  starts_on: string;
  ends_on: string | null;
  active: boolean;
}

export interface CommissionRulePage {
  items: CommissionRule[];
  next_cursor: string | null;
}

/** Replaces the whole rule. */
export interface CommissionRuleInput {
  seller_id?: string;
  product_id?: string;
  rate: string;
  starts_on: string;
  ends_on?: string;
  active: boolean;
}

export interface CommissionRuleFilters {
  seller: string;
  product: string;
  active: "" | "true" | "false";
}

export const NO_COMMISSION_RULE_FILTERS: CommissionRuleFilters = { seller: "", product: "", active: "" };

export interface CommissionStatementSale {
  sale_id: string;
  number: number;
  sold_on: string;
  customer_id: string;
  /** The net value of the lines, less what came back. */
  base: string;
  commission: string;
}

export interface SellerStatement {
  seller_id: string;
  base: string;
  commission: string;
  sales: CommissionStatementSale[];
}

export interface CommissionStatement {
  from: string;
  to: string;
  base: string;
  commission: string;
  sellers: SellerStatement[];
}

export interface EntryInstallment {
  number: number;
  /** `YYYY-MM-DD`. */
  due_date: string;
  /** A decimal string without trailing zeros, in cents at most. */
  amount: string;
  /** How much of it the settlements that are not reversed have settled, a decimal string. */
  settled: string;
}

/** Money owed to the company (`receivable`) or by it (`payable`), split into installments (ADR 0029). */
export interface Entry {
  id: string;
  company_id: string;
  kind: EntryKind;
  description: string;
  party_id: string | null;
  category_id: string;
  cost_center_id: string | null;
  /** `YYYY-MM-DD`: the day the entry belongs to for the managerial income statement. */
  competence_date: string;
  installments: EntryInstallment[];
  /** The sum of the installments, a decimal string. */
  total: string;
  /** The recurrence that generated it, if one did (ADR 0033). */
  recurrence_id: string | null;
  /** The sale whose receivable it is, if one made it (ADR 0039). */
  sale_id: string | null;
  /** The return it refunds, if one made it. */
  sale_return_id: string | null;
  /** The receipt of goods whose payable it is, if one made it (ADR 0053). */
  purchase_receipt_id: string | null;
  /** Cancelled: it counts nowhere and takes no edit or settlement (ADR 0037). */
  voided: boolean;
}

export interface EntryPage {
  items: Entry[];
  next_cursor: string | null;
}

/** Replaces the whole entry: a reference left out is removed. */
export interface EntryInput {
  kind: EntryKind;
  description: string;
  party_id?: string;
  category_id: string;
  cost_center_id?: string;
  competence_date: string;
  installments: { due_date: string; amount: string }[];
}

export interface EntryFilters {
  q: string;
  kind: "" | EntryKind;
  /** The id of a category, or empty for all of them. */
  category: string;
  sort: "-competence_date" | "competence_date";
}

export const NO_ENTRY_FILTERS: EntryFilters = { q: "", kind: "", category: "", sort: "-competence_date" };

export class ApiError extends Error {
  constructor(
    readonly status: number,
    message: string,
  ) {
    super(message);
  }
}

export function errorMessage(failure: unknown): string {
  return failure instanceof ApiError ? failure.message : "Algo deu errado, tente novamente.";
}

let onSessionLost: (() => void) | null = null;

/** Called when a request that needs a session gets 401, e.g. the session expired or was revoked elsewhere. */
export function setSessionLostHandler(handler: (() => void) | null) {
  onSessionLost = handler;
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  if (init.body !== undefined && !headers.has("Content-Type")) {
    headers.set("Content-Type", "application/json");
  }
  const response = await fetch(`/api${path}`, { ...init, headers });
  if (response.status === 204) {
    return undefined as T;
  }
  const text = await response.text();
  const body = parseJson(text);
  // /auth/* answers 401 for ordinary reasons (wrong password, not signed in yet); only other calls mean a lost session.
  if (response.status === 401 && !path.startsWith("/auth/")) {
    onSessionLost?.();
  }
  if (!response.ok) {
    throw failureOf(response, body);
  }
  if (body === undefined) {
    if (text === "") {
      return undefined as T;
    }
    // A 2xx that is not JSON did not come from the API (a proxy aimed at another server, or a
    // fallback serving the app for /api); passing it on as data would crash whatever reads it.
    throw new ApiError(response.status, "Resposta inesperada do servidor: confira se o endereço da API está certo.");
  }
  return body as T;
}

/** What a refused request says, in the server's own words when it said any. */
function failureOf(response: Response, body: unknown): ApiError {
  const message =
    typeof body === "object" && body !== null && "error" in body ? String(body.error) : response.statusText;
  return new ApiError(response.status, message);
}

/** A file the API makes to be saved, with the name it suggests. */
export interface SavedFile {
  blob: Blob;
  filename: string;
}

/**
 * Reads a whole file instead of letting the browser follow a link, so a refusal (a span too big for one package) is told
 * in words, not saved as a file of JSON.
 */
async function fetchFile(path: string, fallbackName: string): Promise<SavedFile> {
  const response = await fetch(`/api${path}`);
  if (!response.ok) {
    if (response.status === 401) {
      onSessionLost?.();
    }
    throw failureOf(response, parseJson(await response.text()));
  }
  // A 2xx that is a page did not come from the API (a fallback serving the app for /api): it would be saved as a file.
  if ((response.headers.get("Content-Type") ?? "").startsWith("text/html")) {
    throw new ApiError(response.status, "Resposta inesperada do servidor: confira se o endereço da API está certo.");
  }
  const named = /filename="([^"]+)"/.exec(response.headers.get("Content-Disposition") ?? "");
  return { blob: await response.blob(), filename: named?.[1] ?? fallbackName };
}

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

const post = <T>(path: string, body?: unknown) =>
  request<T>(path, {
    method: "POST",
    body: body === undefined ? undefined : JSON.stringify(body),
  });

export const api = {
  register: (registration: Registration) => post<Profile>("/auth/register", registration),
  login: (email: string, password: string) => post<Profile>("/auth/login", { email, password }),
  logout: () => post<void>("/auth/logout"),
  /** Sends the signed-in person a new link to confirm their address. */
  requestEmailConfirmation: () => post<void>("/auth/email-confirmation"),
  confirmEmail: (token: string) => post<void>("/auth/email-confirmation/confirm", { token }),
  /** Always answers the same, whether the address has an account or not. */
  requestPasswordReset: (email: string) => post<void>("/auth/password-reset", { email }),
  resetPassword: (token: string, password: string) => post<void>("/auth/password-reset/confirm", { token, password }),
  me: () => request<Profile>("/auth/me"),
  /** A cursor only continues the list that produced it, so pass the same filters with it. */
  listParties: (companyId: string, filters: PartyFilters = NO_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.kind) params.set("kind", filters.kind);
    if (filters.tag.trim()) params.set("tag", filters.tag.trim());
    if (filters.sort !== NO_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<PartyPage>(`/companies/${companyId}/parties${query}`);
  },
  createParty: (companyId: string, party: PartyInput) =>
    post<Party>(`/companies/${companyId}/parties`, party),
  updateParty: (companyId: string, partyId: string, party: PartyInput) =>
    request<Party>(`/companies/${companyId}/parties/${partyId}`, {
      method: "PUT",
      body: JSON.stringify(party),
    }),
  /** Asks a third-party service, so it fails now and then: callers fall back to typing. */
  lookupPostalCode: (companyId: string, postalCode: string) =>
    request<PostalCodeEntry>(
      `/companies/${companyId}/postal-codes/${postalCode.replace(/\D/g, "")}`,
    ),
  /** The file goes as it is, bytes and all: the API says if it is not UTF-8. */
  importParties: (companyId: string, file: File, dryRun: boolean) =>
    request<ImportResult>(
      `/companies/${companyId}/parties/import${dryRun ? "?dry_run=true" : ""}`,
      { method: "POST", headers: { "Content-Type": "text/csv" }, body: file },
    ),
  /** Like `importParties`: the file goes as it is. */
  importProducts: (companyId: string, file: File, dryRun: boolean) =>
    request<ImportResult>(
      `/companies/${companyId}/products/import${dryRun ? "?dry_run=true" : ""}`,
      { method: "POST", headers: { "Content-Type": "text/csv" }, body: file },
    ),
  /** One piece of an entries file, sent as text; `importEntriesInPieces` cuts the file into them. */
  importEntriesPiece: (companyId: string, csv: string, dryRun: boolean) =>
    request<ImportResult>(
      `/companies/${companyId}/entries/import${dryRun ? "?dry_run=true" : ""}`,
      { method: "POST", headers: { "Content-Type": "text/csv" }, body: csv },
    ),
  listMembers: (companyId: string) => request<Member[]>(`/companies/${companyId}/members`),
  changeRole: (companyId: string, userId: string, role: Role) =>
    request<void>(`/companies/${companyId}/members/${userId}`, {
      method: "PATCH",
      body: JSON.stringify({ role }),
    }),
  removeMember: (companyId: string, userId: string) =>
    request<void>(`/companies/${companyId}/members/${userId}`, { method: "DELETE" }),
  listInvitations: (companyId: string) =>
    request<Invitation[]>(`/companies/${companyId}/invitations`),
  invite: (companyId: string, email: string, role: Role) =>
    post<Invited>(`/companies/${companyId}/invitations`, { email, role }),
  revokeInvitation: (companyId: string, invitationId: string) =>
    request<void>(`/companies/${companyId}/invitations/${invitationId}`, { method: "DELETE" }),
  /** Needs no session: the person may not have an account yet. */
  previewInvitation: (token: string) =>
    request<InvitationPreview>(`/invitations/${encodeURIComponent(token)}`),
  acceptInvitation: (token: string) =>
    post<{ company_id: string }>(`/invitations/${encodeURIComponent(token)}/accept`),
  partyHistory: (companyId: string, partyId: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/parties/${partyId}/history`),
  /** Asks the registry again about the party's CNPJ; resolves with the party and its new answer. */
  verifyDocument: (companyId: string, partyId: string) =>
    post<Party>(`/companies/${companyId}/parties/${partyId}/document-check`),
  /** As `listParties`: a cursor only continues the list that produced it. */
  listProducts: (companyId: string, filters: ProductFilters = NO_PRODUCT_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.kind) params.set("kind", filters.kind);
    if (filters.active) params.set("active", filters.active);
    if (filters.sort !== NO_PRODUCT_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<ProductPage>(`/companies/${companyId}/products${query}`);
  },
  createProduct: (companyId: string, product: ProductInput) =>
    post<Product>(`/companies/${companyId}/products`, product),
  updateProduct: (companyId: string, productId: string, product: ProductInput) =>
    request<Product>(`/companies/${companyId}/products/${productId}`, {
      method: "PUT",
      body: JSON.stringify(product),
    }),
  productHistory: (companyId: string, productId: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/products/${productId}/history`),
  listCostCenters: (
    companyId: string,
    filters: CostCenterFilters = NO_COST_CENTER_FILTERS,
    after?: string,
  ) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.active) params.set("active", filters.active);
    if (filters.sort !== NO_COST_CENTER_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<CostCenterPage>(`/companies/${companyId}/cost-centers${query}`);
  },
  createCostCenter: (companyId: string, input: CostCenterInput) =>
    post<CostCenter>(`/companies/${companyId}/cost-centers`, input),
  updateCostCenter: (companyId: string, id: string, input: CostCenterInput) =>
    request<CostCenter>(`/companies/${companyId}/cost-centers/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  costCenterHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/cost-centers/${id}/history`),
  listBankAccounts: (
    companyId: string,
    filters: BankAccountFilters = NO_BANK_ACCOUNT_FILTERS,
    after?: string,
  ) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.kind) params.set("kind", filters.kind);
    if (filters.active) params.set("active", filters.active);
    if (filters.sort !== NO_BANK_ACCOUNT_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<BankAccountPage>(`/companies/${companyId}/bank-accounts${query}`);
  },
  createBankAccount: (companyId: string, input: BankAccountInput) =>
    post<BankAccount>(`/companies/${companyId}/bank-accounts`, input),
  updateBankAccount: (companyId: string, id: string, input: BankAccountInput) =>
    request<BankAccount>(`/companies/${companyId}/bank-accounts/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  bankAccountHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/bank-accounts/${id}/history`),
  listPaymentMethods: (
    companyId: string,
    filters: PaymentMethodFilters = NO_PAYMENT_METHOD_FILTERS,
    after?: string,
  ) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.kind) params.set("kind", filters.kind);
    if (filters.active) params.set("active", filters.active);
    if (filters.sort !== NO_PAYMENT_METHOD_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<PaymentMethodPage>(`/companies/${companyId}/payment-methods${query}`);
  },
  createPaymentMethod: (companyId: string, input: PaymentMethodInput) =>
    post<PaymentMethod>(`/companies/${companyId}/payment-methods`, input),
  updatePaymentMethod: (companyId: string, id: string, input: PaymentMethodInput) =>
    request<PaymentMethod>(`/companies/${companyId}/payment-methods/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  paymentMethodHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/payment-methods/${id}/history`),
  listPaymentTerms: (
    companyId: string,
    filters: PaymentTermFilters = NO_PAYMENT_TERM_FILTERS,
    after?: string,
  ) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.active) params.set("active", filters.active);
    if (filters.sort !== NO_PAYMENT_TERM_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<PaymentTermPage>(`/companies/${companyId}/payment-terms${query}`);
  },
  listPriceLists: (companyId: string, filters: PriceListFilters = NO_PRICE_LIST_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.active) params.set("active", filters.active);
    if (filters.sort !== NO_PRICE_LIST_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<PriceListPage>(`/companies/${companyId}/price-lists${query}`);
  },
  createPriceList: (companyId: string, input: PriceListInput) =>
    post<PriceList>(`/companies/${companyId}/price-lists`, input),
  updatePriceList: (companyId: string, id: string, input: PriceListInput) =>
    request<PriceList>(`/companies/${companyId}/price-lists/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  priceListHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/price-lists/${id}/history`),
  createPaymentTerm: (companyId: string, input: PaymentTermInput) =>
    post<PaymentTerm>(`/companies/${companyId}/payment-terms`, input),
  updatePaymentTerm: (companyId: string, id: string, input: PaymentTermInput) =>
    request<PaymentTerm>(`/companies/${companyId}/payment-terms/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  paymentTermHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/payment-terms/${id}/history`),
  listEntries: (companyId: string, filters: EntryFilters = NO_ENTRY_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.kind) params.set("kind", filters.kind);
    if (filters.category) params.set("category", filters.category);
    if (filters.sort !== NO_ENTRY_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<EntryPage>(`/companies/${companyId}/entries${query}`);
  },
  createEntry: (companyId: string, input: EntryInput) =>
    post<Entry>(`/companies/${companyId}/entries`, input),
  updateEntry: (companyId: string, id: string, input: EntryInput) =>
    request<Entry>(`/companies/${companyId}/entries/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  getEntry: (companyId: string, id: string) =>
    request<Entry>(`/companies/${companyId}/entries/${id}`),
  entryHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/entries/${id}/history`),
  settleInstallment: (companyId: string, entryId: string, input: SettlementInput) =>
    post<Settlement>(`/companies/${companyId}/entries/${entryId}/settlements`, input),
  listSettlements: (companyId: string, entryId: string) =>
    request<Settlement[]>(`/companies/${companyId}/entries/${entryId}/settlements`),
  /** Undoes a settlement: it stops counting and stays listed as reversed. */
  reverseSettlement: (companyId: string, entryId: string, settlementId: string) =>
    request<void>(`/companies/${companyId}/entries/${entryId}/settlements/${settlementId}`, {
      method: "DELETE",
    }),
  listAttachments: (companyId: string, entryId: string) =>
    request<{ items: Attachment[] }>(`/companies/${companyId}/entries/${entryId}/attachments`),
  /** The file travels as the body of the request, with its own type; its name is in the query. */
  attachFile: (companyId: string, entryId: string, file: File) =>
    request<Attachment>(
      `/companies/${companyId}/entries/${entryId}/attachments?name=${encodeURIComponent(file.name)}`,
      {
        method: "POST",
        body: file,
        headers: { "Content-Type": contentTypeOf(file) },
      },
    ),
  /** Where the file is downloaded from: the session cookie goes with a plain link. */
  attachmentUrl: (companyId: string, entryId: string, attachmentId: string) =>
    `/api/companies/${companyId}/entries/${entryId}/attachments/${attachmentId}`,
  removeAttachment: (companyId: string, entryId: string, attachmentId: string) =>
    request<void>(`/companies/${companyId}/entries/${entryId}/attachments/${attachmentId}`, {
      method: "DELETE",
    }),
  getPackageSchedule: (companyId: string) =>
    request<PackageSchedule>(`/companies/${companyId}/accounting/package-schedule`),
  replacePackageSchedule: (companyId: string, input: { enabled: boolean; day: number }) =>
    request<PackageSchedule>(`/companies/${companyId}/accounting/package-schedule`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  /** The accountant's files (ADR 0071): `from` and `to` are days, both included. */
  exportFile: (companyId: string, file: ExportFile, from: string, to: string, format: ExportFormat) =>
    fetchFile(
      `/companies/${companyId}/accounting/exports/${file}?${new URLSearchParams({ from, to, format })}`,
      `${file}-${from}-a-${to}.${file === "package" ? "zip" : format}`,
    ),
  /** The provider charges are made at, or `null` when the operator configured none. */
  getPeriodLock: (companyId: string) =>
    request<PeriodLock>(`/companies/${companyId}/accounting/period-lock`),
  /** Closes through a month's last day, or reopens with an earlier day (or none) and a reason. */
  replacePeriodLock: (companyId: string, lock: { locked_through: string | null; reason?: string }) =>
    request<PeriodLock>(`/companies/${companyId}/accounting/period-lock`, {
      method: "PUT",
      body: JSON.stringify(lock),
    }),
  periodLockHistory: (companyId: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/accounting/period-lock/history`),
  /** The gateway this installation issues through, or `null` when the operator configured none. */
  getFiscalGateway: (companyId: string) =>
    request<{ gateway: string | null }>(`/companies/${companyId}/fiscal/gateway`),
  /** `null` while the company has not set them: no document can be issued then. */
  getFiscalSettings: (companyId: string) =>
    request<FiscalSettings | null>(`/companies/${companyId}/fiscal/settings`),
  replaceFiscalSettings: (companyId: string, settings: FiscalSettings) =>
    request<FiscalSettings>(`/companies/${companyId}/fiscal/settings`, {
      method: "PUT",
      body: JSON.stringify(settings),
    }),
  listFiscalDocuments: (companyId: string, filters: FiscalDocumentFilters, after?: string) => {
    const params = new URLSearchParams();
    if (filters.status) params.set("status", filters.status);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<FiscalDocumentPage>(`/companies/${companyId}/fiscal/documents${query}`);
  },
  getFiscalDocument: (companyId: string, id: string) =>
    request<FiscalDocument>(`/companies/${companyId}/fiscal/documents/${id}`),
  issueFiscalDocument: (companyId: string, choices: IssueChoices) =>
    post<FiscalDocument>(`/companies/${companyId}/fiscal/documents`, choices),
  resendFiscalDocument: (companyId: string, id: string, choices: IssueChoices) =>
    post<FiscalDocument>(`/companies/${companyId}/fiscal/documents/${id}/resend`, choices),
  refreshFiscalDocument: (companyId: string, id: string) =>
    post<FiscalDocument>(`/companies/${companyId}/fiscal/documents/${id}/refresh`),
  cancelFiscalDocument: (companyId: string, id: string, reason: string) =>
    post<FiscalDocument>(`/companies/${companyId}/fiscal/documents/${id}/cancel`, { reason }),
  fiscalDocumentHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/fiscal/documents/${id}/history`),
  fiscalDocumentFile: (companyId: string, id: string, kind: FiscalFileKind) =>
    fetchFile(`/companies/${companyId}/fiscal/documents/${id}/files/${kind}`, `${kind}-${id}.${kind === "pdf" ? "pdf" : "xml"}`),
  listCorrections: (companyId: string, id: string) =>
    request<FiscalEvent[]>(`/companies/${companyId}/fiscal/documents/${id}/corrections`),
  sendCorrection: (companyId: string, id: string, text: string) =>
    post<FiscalEvent>(`/companies/${companyId}/fiscal/documents/${id}/corrections`, { text }),
  listInvalidations: (companyId: string) =>
    request<FiscalEvent[]>(`/companies/${companyId}/fiscal/invalidations`),
  invalidateNumbers: (companyId: string, input: { series: number; first: number; last: number; reason: string }) =>
    post<FiscalEvent>(`/companies/${companyId}/fiscal/invalidations`, input),
  abandonFiscalEvent: (companyId: string, id: string) =>
    post<FiscalEvent>(`/companies/${companyId}/fiscal/events/${id}/abandon`),
  fiscalEventFile: (companyId: string, id: string) =>
    fetchFile(`/companies/${companyId}/fiscal/events/${id}/file`, `event-${id}.xml`),
  fiscalSettingsHistory: (companyId: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/fiscal/settings/history`),
  getCollection: (companyId: string) =>
    request<{ provider: string | null }>(`/companies/${companyId}/collection`),
  getCollectionSettings: (companyId: string) =>
    request<CollectionSettings>(`/companies/${companyId}/collection/settings`),
  replaceCollectionSettings: (
    companyId: string,
    settings: { bank_account_id: string; fee_category_id: string | null },
  ) =>
    request<CollectionSettings>(`/companies/${companyId}/collection/settings`, {
      method: "PUT",
      body: JSON.stringify(settings),
    }),
  /** Makes a link to the charge's payment page; the address is shown once, only its digest is kept. */
  newPaymentLink: (companyId: string, entryId: string, chargeId: string) =>
    request<{ url: string }>(
      `/companies/${companyId}/entries/${entryId}/charges/${chargeId}/link`,
      { method: "POST" },
    ),
  publicCharge: (token: string) => request<PublicCharge>(`/public/charges/${token}`),
  getDelinquency: (companyId: string, on: string) =>
    request<Delinquency>(`/companies/${companyId}/delinquency?on=${on}`),
  getDunningPlan: (companyId: string) =>
    request<DunningPlan>(`/companies/${companyId}/collection/dunning`),
  replaceDunningPlan: (companyId: string, plan: { enabled: boolean; rules: DunningRule[] }) =>
    request<DunningPlan>(`/companies/${companyId}/collection/dunning`, {
      method: "PUT",
      body: JSON.stringify(plan),
    }),
  listReminders: (companyId: string, entryId: string) =>
    request<{ items: Reminder[] }>(`/companies/${companyId}/entries/${entryId}/notifications`),
  listCharges: (companyId: string, entryId: string) =>
    request<{ items: Charge[] }>(`/companies/${companyId}/entries/${entryId}/charges`),
  issueCharge: (companyId: string, entryId: string, input: ChargeInput) =>
    request<Charge>(`/companies/${companyId}/entries/${entryId}/charges`, {
      method: "POST",
      body: JSON.stringify(input),
    }),
  cancelCharge: (companyId: string, entryId: string, chargeId: string) =>
    request<Charge>(`/companies/${companyId}/entries/${entryId}/charges/${chargeId}/cancel`, {
      method: "POST",
    }),
  listRecurrences: (
    companyId: string,
    filters: RecurrenceFilters = NO_RECURRENCE_FILTERS,
    after?: string,
  ) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.kind) params.set("kind", filters.kind);
    if (filters.active) params.set("active", filters.active);
    if (filters.sort !== NO_RECURRENCE_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<RecurrencePage>(`/companies/${companyId}/recurrences${query}`);
  },
  createRecurrence: (companyId: string, input: RecurrenceInput) =>
    post<Recurrence>(`/companies/${companyId}/recurrences`, input),
  updateRecurrence: (companyId: string, id: string, input: RecurrenceInput) =>
    request<Recurrence>(`/companies/${companyId}/recurrences/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  recurrenceHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/recurrences/${id}/history`),
  listQuotes: (companyId: string, filters: QuoteFilters = NO_QUOTE_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.status) params.set("status", filters.status);
    if (filters.sort !== NO_QUOTE_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<QuotePage>(`/companies/${companyId}/quotes${query}`);
  },
  createQuote: (companyId: string, input: QuoteInput) =>
    post<Quote>(`/companies/${companyId}/quotes`, input),
  updateQuote: (companyId: string, id: string, input: QuoteInput) =>
    request<Quote>(`/companies/${companyId}/quotes/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  quoteHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/quotes/${id}/history`),
  sendQuote: (companyId: string, id: string, to: string) =>
    post<SentQuote>(`/companies/${companyId}/quotes/${id}/send`, { to }),
  /** Where the PDF is: the session cookie goes with a plain link. */
  quotePdfUrl: (companyId: string, id: string) => `/api/companies/${companyId}/quotes/${id}/pdf`,
  /** The three below need no session: the token in the link is the secret. */
  publicQuote: (token: string) => request<PublicQuote>(`/public/quotes/${token}`),
  publicQuotePdfUrl: (token: string) => `/api/public/quotes/${token}/pdf`,
  approveQuote: (token: string) => post<PublicQuote>(`/public/quotes/${token}/approve`),
  declineQuote: (token: string) => post<PublicQuote>(`/public/quotes/${token}/decline`),
  listContracts: (companyId: string, filters: ContractFilters = NO_CONTRACT_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.status) params.set("status", filters.status);
    if (filters.sort !== NO_CONTRACT_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<ContractPage>(`/companies/${companyId}/contracts${query}`);
  },
  createContract: (companyId: string, input: ContractInput) =>
    post<Contract>(`/companies/${companyId}/contracts`, input),
  updateContract: (companyId: string, id: string, input: ContractInput) =>
    request<Contract>(`/companies/${companyId}/contracts/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  adjustContract: (companyId: string, id: string, percent: string) =>
    post<Contract>(`/companies/${companyId}/contracts/${id}/adjust`, { percent }),
  terminateContract: (companyId: string, id: string, on: string) =>
    post<Contract>(`/companies/${companyId}/contracts/${id}/terminate`, { on }),
  contractHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/contracts/${id}/history`),
  listCommissionRules: (companyId: string, filters: CommissionRuleFilters = NO_COMMISSION_RULE_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.seller) params.set("seller", filters.seller);
    if (filters.product) params.set("product", filters.product);
    if (filters.active) params.set("active", filters.active);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<CommissionRulePage>(`/companies/${companyId}/commission-rules${query}`);
  },
  createCommissionRule: (companyId: string, input: CommissionRuleInput) =>
    post<CommissionRule>(`/companies/${companyId}/commission-rules`, input),
  updateCommissionRule: (companyId: string, id: string, input: CommissionRuleInput) =>
    request<CommissionRule>(`/companies/${companyId}/commission-rules/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  commissionRuleHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/commission-rules/${id}/history`),
  commissionStatement: (companyId: string, from: string, to: string, seller: string) => {
    const params = new URLSearchParams({ from, to });
    if (seller) params.set("seller", seller);
    return request<CommissionStatement>(`/companies/${companyId}/commissions?${params}`);
  },
  convertQuote: (companyId: string, quoteId: string, input: ConversionInput) =>
    post<Sale>(`/companies/${companyId}/quotes/${quoteId}/convert`, input),
  listSales: (companyId: string, filters: SaleFilters = NO_SALE_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.status) params.set("status", filters.status);
    if (filters.sort !== NO_SALE_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<SalePage>(`/companies/${companyId}/sales${query}`);
  },
  createSale: (companyId: string, input: SaleInput) =>
    post<Sale>(`/companies/${companyId}/sales`, input),
  updateSale: (companyId: string, id: string, input: SaleInput) =>
    request<Sale>(`/companies/${companyId}/sales/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  saleHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/sales/${id}/history`),
  completeSale: (companyId: string, id: string) =>
    post<Sale>(`/companies/${companyId}/sales/${id}/complete`),
  cancelSale: (companyId: string, id: string) =>
    post<Sale>(`/companies/${companyId}/sales/${id}/cancel`),
  listReturns: (companyId: string, id: string) =>
    request<{ items: SaleReturn[] }>(`/companies/${companyId}/sales/${id}/returns`),
  createReturn: (companyId: string, id: string, input: ReturnInput) =>
    post<SaleReturn>(`/companies/${companyId}/sales/${id}/returns`, input),
  listPurchaseOrders: (companyId: string, filters: PurchaseOrderFilters = NO_PURCHASE_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.status) params.set("status", filters.status);
    if (filters.from) params.set("from", filters.from);
    if (filters.to) params.set("to", filters.to);
    if (filters.sort !== NO_PURCHASE_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<PurchaseOrderPage>(`/companies/${companyId}/purchase-orders${query}`);
  },
  sendPurchaseOrder: (companyId: string, id: string, to: string) =>
    post<Mailed>(`/companies/${companyId}/purchase-orders/${id}/send`, to ? { to } : {}),
  purchaseOrderMails: (companyId: string, id: string) =>
    request<{ items: SentMail[] }>(`/companies/${companyId}/purchase-orders/${id}/mails`),
  paymentReceiptMails: (companyId: string, entryId: string, settlementId: string) =>
    request<{ items: SentMail[] }>(`/companies/${companyId}/entries/${entryId}/settlements/${settlementId}/mails`),
  sendPaymentReceipt: (companyId: string, entryId: string, settlementId: string, to: string) =>
    post<Mailed>(`/companies/${companyId}/entries/${entryId}/settlements/${settlementId}/receipt`, to ? { to } : {}),
  getPurchaseStatusCounts: (companyId: string, filters: Pick<PurchaseOrderFilters, "q" | "from" | "to">) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.from) params.set("from", filters.from);
    if (filters.to) params.set("to", filters.to);
    const query = params.size > 0 ? `?${params}` : "";
    return request<PurchaseStatusCounts>(`/companies/${companyId}/purchase-orders/status-counts${query}`);
  },
  getPurchaseOrder: (companyId: string, id: string) =>
    request<PurchaseOrder>(`/companies/${companyId}/purchase-orders/${id}`),
  createPurchaseOrder: (companyId: string, input: PurchaseOrderInput) =>
    post<PurchaseOrder>(`/companies/${companyId}/purchase-orders`, input),
  updatePurchaseOrder: (companyId: string, id: string, input: PurchaseOrderInput) =>
    request<PurchaseOrder>(`/companies/${companyId}/purchase-orders/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  purchaseOrderHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/purchase-orders/${id}/history`),
  approvePurchaseOrder: (companyId: string, id: string) =>
    post<PurchaseOrder>(`/companies/${companyId}/purchase-orders/${id}/approve`),
  cancelPurchaseOrder: (companyId: string, id: string) =>
    post<PurchaseOrder>(`/companies/${companyId}/purchase-orders/${id}/cancel`),
  closePurchaseOrder: (companyId: string, id: string) =>
    post<PurchaseOrder>(`/companies/${companyId}/purchase-orders/${id}/close`),
  listReceipts: (companyId: string, id: string) =>
    request<{ items: PurchaseReceipt[] }>(`/companies/${companyId}/purchase-orders/${id}/receipts`),
  receivePurchaseOrder: (companyId: string, id: string, input: ReceiptInput) =>
    post<PurchaseReceipt>(`/companies/${companyId}/purchase-orders/${id}/receipts`, input),
  finishReceipt: (companyId: string, id: string, receiptId: string) =>
    post<PurchaseReceipt>(`/companies/${companyId}/purchase-orders/${id}/receipts/${receiptId}/finish`),
  listStockPositions: (companyId: string, filters: StockPositionFilters = NO_POSITION_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.situation) params.set("situation", filters.situation);
    if (filters.on) params.set("on", filters.on);
    if (filters.sort !== NO_POSITION_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<StockPositionPage>(`/companies/${companyId}/stock/positions${query}`);
  },
  getStockPositionSummary: (companyId: string, filters: Pick<StockPositionFilters, "q" | "on">) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.on) params.set("on", filters.on);
    const query = params.size > 0 ? `?${params}` : "";
    return request<StockPositionSummary>(`/companies/${companyId}/stock/positions/summary${query}`);
  },
  listStockMovements: (companyId: string, filters: StockMovementFilters = NO_MOVEMENT_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.product) params.set("product", filters.product);
    if (filters.kind) params.set("kind", filters.kind);
    if (filters.from) params.set("from", filters.from);
    if (filters.to) params.set("to", filters.to);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<StockMovementPage>(`/companies/${companyId}/stock/movements${query}`);
  },
  getStockKindCounts: (companyId: string, filters: Pick<StockMovementFilters, "product" | "from" | "to">) => {
    const params = new URLSearchParams();
    if (filters.product) params.set("product", filters.product);
    if (filters.from) params.set("from", filters.from);
    if (filters.to) params.set("to", filters.to);
    const query = params.size > 0 ? `?${params}` : "";
    return request<StockKindCounts>(`/companies/${companyId}/stock/movements/kind-counts${query}`);
  },
  recordStockMovement: (companyId: string, input: StockMovementInput) =>
    post<StockMovement>(`/companies/${companyId}/stock/movements`, input),
  reverseStockMovement: (companyId: string, id: string, input: StockReverseInput) =>
    post<StockMovement>(`/companies/${companyId}/stock/movements/${id}/reverse`, input),
  getStockAlerts: (companyId: string) => request<{ items: StockAlert[] }>(`/companies/${companyId}/stock/alerts`),
  getStockSettings: (companyId: string) => request<StockSettings>(`/companies/${companyId}/stock/settings`),
  putStockSettings: (companyId: string, settings: StockSettings) =>
    request<StockSettings>(`/companies/${companyId}/stock/settings`, {
      method: "PUT",
      body: JSON.stringify(settings),
    }),
  stockSettingsHistory: (companyId: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/stock/settings/history`),
  listInventoryCounts: (companyId: string, filters: InventoryCountFilters = NO_COUNT_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.q) params.set("q", filters.q);
    if (filters.status) params.set("status", filters.status);
    if (filters.sort !== NO_COUNT_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<InventoryCountPage>(`/companies/${companyId}/inventory-counts${query}`);
  },
  getInventoryCount: (companyId: string, id: string) =>
    request<InventoryCount>(`/companies/${companyId}/inventory-counts/${id}`),
  createInventoryCount: (companyId: string, input: InventoryCountInput) =>
    post<InventoryCount>(`/companies/${companyId}/inventory-counts`, input),
  updateInventoryCount: (companyId: string, id: string, input: InventoryCountInput) =>
    request<InventoryCount>(`/companies/${companyId}/inventory-counts/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  applyInventoryCount: (companyId: string, id: string) =>
    post<InventoryCount>(`/companies/${companyId}/inventory-counts/${id}/apply`),
  cancelInventoryCount: (companyId: string, id: string) =>
    post<InventoryCount>(`/companies/${companyId}/inventory-counts/${id}/cancel`),
  inventoryCountHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/inventory-counts/${id}/history`),
  /** The XML goes as it is, as the body: the API reads and checks it. */
  previewPurchaseImport: (companyId: string, file: File) =>
    request<InvoicePreview>(`/companies/${companyId}/purchase-imports/preview`, {
      method: "POST",
      headers: { "Content-Type": "application/xml" },
      body: file,
    }),
  enterPurchaseImport: (companyId: string, file: File, params: InvoiceEntryParams) => {
    const query = new URLSearchParams({ category_id: params.category_id });
    if (params.cost_center_id) query.set("cost_center_id", params.cost_center_id);
    if (params.payment_term_id) query.set("payment_term_id", params.payment_term_id);
    if (params.received_on) query.set("received_on", params.received_on);
    if (params.supplier_id) query.set("supplier_id", params.supplier_id);
    if (params.products.length > 0) query.set("products", params.products.join(","));
    return request<InvoiceEntry>(`/companies/${companyId}/purchase-imports?${query}`, {
      method: "POST",
      headers: { "Content-Type": "application/xml" },
      body: file,
    });
  },
  stockValuation: (companyId: string, on: string) =>
    request<StockValuation>(`/companies/${companyId}/stock-reports/valuation?${new URLSearchParams({ on })}`),
  stockMovementSummary: (companyId: string, from: string, to: string) =>
    request<StockMovementSummary>(`/companies/${companyId}/stock-reports/movements?${new URLSearchParams({ from, to })}`),
  /** The OFX file goes as it is, bytes and all: the API reads the charset. */
  importStatement: (companyId: string, accountId: string, file: File, dryRun: boolean) =>
    request<ImportResult>(
      `/companies/${companyId}/bank-accounts/${accountId}/statement-import${dryRun ? "?dry_run=true" : ""}`,
      { method: "POST", headers: { "Content-Type": "application/x-ofx" }, body: file },
    ),
  /** Newest day first; a cursor only continues the list of the same filters. */
  listStatementLines: (
    companyId: string,
    filters: StatementLineFilters = NO_STATEMENT_LINE_FILTERS,
    after?: string,
  ) => {
    const params = new URLSearchParams();
    if (filters.account) params.set("account", filters.account);
    if (filters.status) params.set("status", filters.status);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<StatementLinePage>(`/companies/${companyId}/statement-lines${query}`);
  },
  statementLineSuggestions: (companyId: string, id: string) =>
    request<LineSuggestion[]>(`/companies/${companyId}/statement-lines/${id}/suggestions`),
  matchStatementLine: (
    companyId: string,
    id: string,
    target: { settlement_id: string } | { transfer_id: string },
  ) => post<StatementLine>(`/companies/${companyId}/statement-lines/${id}/match`, target),
  unmatchStatementLine: (companyId: string, id: string) =>
    post<StatementLine>(`/companies/${companyId}/statement-lines/${id}/unmatch`),
  ignoreStatementLine: (companyId: string, id: string) =>
    post<StatementLine>(`/companies/${companyId}/statement-lines/${id}/ignore`),
  restoreStatementLine: (companyId: string, id: string) =>
    post<StatementLine>(`/companies/${companyId}/statement-lines/${id}/restore`),
  createEntryFromLine: (companyId: string, id: string, input: EntryFromLineInput) =>
    post<{ line: StatementLine; entry_id: string; settlement_id: string }>(
      `/companies/${companyId}/statement-lines/${id}/create-entry`,
      input,
    ),
  /** Newest first; a cursor only continues the list of the same filters. */
  listTransfers: (companyId: string, filters: TransferFilters = NO_TRANSFER_FILTERS, after?: string) => {
    const params = new URLSearchParams();
    if (filters.account) params.set("account", filters.account);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<TransferPage>(`/companies/${companyId}/transfers${query}`);
  },
  createTransfer: (companyId: string, input: TransferInput) =>
    post<Transfer>(`/companies/${companyId}/transfers`, input),
  /** Undoes a transfer: it stops counting and stays listed as reversed. */
  reverseTransfer: (companyId: string, id: string) =>
    request<void>(`/companies/${companyId}/transfers/${id}`, { method: "DELETE" }),
  /** `on` is today where the person is: the server does not know their zone (ADR 0031). */
  getBalances: (companyId: string, on: string) =>
    request<Balances>(`/companies/${companyId}/balances?on=${on}`),
  setOpeningBalance: (companyId: string, accountId: string, input: OpeningBalance) =>
    request<OpeningBalance>(`/companies/${companyId}/bank-accounts/${accountId}/opening-balance`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  salesReport: (companyId: string, by: SalesGroup, from: string, to: string) =>
    request<SalesReport>(`/companies/${companyId}/sales-reports?${new URLSearchParams({ by, from, to })}`),
  /** `costCenter` is an id, `none` for the entries that have none, or empty for all. */
  incomeStatement: (companyId: string, from: string, to: string, by: DreBy, costCenter: string) => {
    const params = new URLSearchParams({ from, to, by });
    if (costCenter) params.set("cost_center", costCenter);
    return request<Dre>(`/companies/${companyId}/dre?${params}`);
  },
  entriesReport: (companyId: string, filters: EntriesReportFilters) =>
    request<EntriesReport>(`/companies/${companyId}/reports/entries?${new URLSearchParams({ ...filters })}`),
  /** `on` is today where the person is: the server does not know their zone. */
  overdueReport: (companyId: string, on: string, kind: "" | EntryKind) => {
    const params = new URLSearchParams({ on });
    if (kind) params.set("kind", kind);
    return request<OverdueReport>(`/companies/${companyId}/reports/overdue?${params}`);
  },
  getCashFlow: (companyId: string, filters: CashFlowFilters) => {
    const params = new URLSearchParams({ from: filters.from, to: filters.to, granularity: filters.granularity });
    if (filters.account) params.set("account", filters.account);
    return request<CashFlow>(`/companies/${companyId}/cash-flow?${params}`);
  },
  /** The whole tree, flat and unpaged. */
  listCategories: (companyId: string) =>
    request<{ items: Category[] }>(`/companies/${companyId}/categories`),
  createCategory: (companyId: string, input: NewCategory) =>
    post<Category>(`/companies/${companyId}/categories`, input),
  /** Only the name and the active flag change; the parent and the groups are fixed. */
  changeCategory: (companyId: string, id: string, input: { name: string; active: boolean }) =>
    request<Category>(`/companies/${companyId}/categories/${id}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  /** For a company that has no categories yet. */
  applyDefaultCategories: (companyId: string) =>
    post<{ items: Category[] }>(`/companies/${companyId}/categories/default-template`),
  categoryHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/categories/${id}/history`),
};
