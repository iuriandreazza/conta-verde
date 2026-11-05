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

export interface PartyPage {
  items: Party[];
  next_cursor: string | null;
}

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
  listParties: (companyId: string, after?: string) => {
    const query = after ? `?${new URLSearchParams({ after })}` : "";
    return request<PartyPage>(`/companies/${companyId}/parties${query}`);
  },
  createParty: (companyId: string, party: NewParty) =>
    post<Party>(`/companies/${companyId}/parties`, party),
};
