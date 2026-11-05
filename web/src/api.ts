export interface User {
  id: string;
  name: string;
  email: string;
}

export interface Company {
  id: string;
  name: string;
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

export interface Party {
  id: string;
  company_id: string;
  name: string;
  document: string | null;
  email: string | null;
  is_customer: boolean;
  is_supplier: boolean;
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

export interface PartyPage {
  items: Party[];
  next_cursor: string | null;
}

/** How a list is narrowed and ordered; the same three parameters every list endpoint takes. */
export interface PartyFilters {
  /** Text found in the name, e-mail or document. */
  q: string;
  kind: "" | "customer" | "supplier";
  /** A leading `-` means descending. */
  sort: "name" | "-name";
}

export const NO_FILTERS: PartyFilters = { q: "", kind: "", sort: "name" };

export interface NewParty {
  name: string;
  document?: string;
  email?: string;
  is_customer: boolean;
  is_supplier: boolean;
}

export interface Registration {
  name: string;
  email: string;
  password: string;
  company_name: string;
}

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
  if (init.body !== undefined) {
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
    if (filters.sort !== NO_FILTERS.sort) params.set("sort", filters.sort);
    if (after) params.set("after", after);
    const query = params.size > 0 ? `?${params}` : "";
    return request<PartyPage>(`/companies/${companyId}/parties${query}`);
  },
  createParty: (companyId: string, party: NewParty) =>
    post<Party>(`/companies/${companyId}/parties`, party),
  partyHistory: (companyId: string, partyId: string) =>
    request<HistoryEntry[]>(`/companies/${companyId}/parties/${partyId}/history`),
  /** Asks the registry again about the party's CNPJ; resolves with the party and its new answer. */
  verifyDocument: (companyId: string, partyId: string) =>
    post<Party>(`/companies/${companyId}/parties/${partyId}/document-check`),
};
