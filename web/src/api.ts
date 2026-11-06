export interface User {
  id: string;
  name: string;
  email: string;
}

export type Role = "owner" | "admin" | "financial" | "sales" | "stock" | "accountant" | "viewer";

/** A part of the product a permission is about (ADR 0026). */
export type Area = "parties" | "products" | "finance" | "members";

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
  /** Rows whose document or SKU is already registered: never overwritten. */
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

export type EntryKind = "receivable" | "payable";

export interface EntryInstallment {
  number: number;
  /** `YYYY-MM-DD`. */
  due_date: string;
  /** A decimal string without trailing zeros, in cents at most. */
  amount: string;
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
  const body: unknown = await response.json().catch(() => null);
  // /auth/* answers 401 for ordinary reasons (wrong password, not signed in yet); only other calls mean a lost session.
  if (response.status === 401 && !path.startsWith("/auth/")) {
    onSessionLost?.();
  }
  if (!response.ok) {
    const message =
      typeof body === "object" && body !== null && "error" in body
        ? String(body.error)
        : response.statusText;
    throw new ApiError(response.status, message);
  }
  return body as T;
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
  entryHistory: (companyId: string, id: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/entries/${id}/history`),
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
