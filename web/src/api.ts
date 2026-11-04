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

export interface Party {
  id: string;
  company_id: string;
  name: string;
  document: string | null;
  email: string | null;
  is_customer: boolean;
  is_supplier: boolean;
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
    const query = new URLSearchParams();
    if (after) {
      query.set("after", after);
    }
    return request<PartyPage>(`/companies/${companyId}/parties?${query}`);
  },
  createParty: (companyId: string, party: NewParty) =>
    post<Party>(`/companies/${companyId}/parties`, party),
};
