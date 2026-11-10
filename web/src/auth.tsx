import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { ApiError, api, setSessionLostHandler, type Profile, type Registration } from "./api";
import { clearOfflineData, syncIdentity } from "./pwa/client";

type Session =
  | { status: "loading" }
  | { status: "anonymous" }
  | { status: "authenticated"; profile: Profile };

interface AuthContextValue {
  session: Session;
  signIn: (email: string, password: string) => Promise<void>;
  signUp: (registration: Registration) => Promise<Profile>;
  signOut: () => Promise<void>;
  /** Reads the profile again, for when what the person belongs to changed (an accepted invitation). */
  refresh: () => Promise<void>;
}

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<Session>({ status: "loading" });

  useEffect(() => {
    api
      .me()
      .then((profile) => setSession({ status: "authenticated", profile }))
      .catch((error: unknown) => {
        // A 401 just means "signed out"; anything else is a real failure worth leaving a trace of.
        if (!(error instanceof ApiError && error.status === 401)) {
          console.error("could not load the session", error);
        }
        setSession({ status: "anonymous" });
      });
  }, []);

  useEffect(() => {
    // A lost session is a device that must not keep the company's data.
    setSessionLostHandler(() => {
      void clearOfflineData();
      setSession({ status: "anonymous" });
    });
    return () => setSessionLostHandler(null);
  }, []);

  // The worker is told who is signed in (it cannot read the HttpOnly cookie), whenever that is known or changes.
  const userId = session.status === "authenticated" ? session.profile.user.id : session.status === "anonymous" ? null : undefined;
  useEffect(() => {
    if (userId !== undefined) void syncIdentity(userId);
  }, [userId]);

  const signIn = useCallback(async (email: string, password: string) => {
    const profile = await api.login(email, password);
    setSession({ status: "authenticated", profile });
  }, []);

  const signUp = useCallback(async (registration: Registration) => {
    const profile = await api.register(registration);
    setSession({ status: "authenticated", profile });
    return profile;
  }, []);

  const signOut = useCallback(async () => {
    // Clearing first and calling the network second: signing out offline still clears the device.
    await clearOfflineData();
    await api.logout();
    setSession({ status: "anonymous" });
  }, []);

  const refresh = useCallback(async () => {
    const profile = await api.me();
    setSession({ status: "authenticated", profile });
  }, []);

  const value = useMemo(
    () => ({ session, signIn, signUp, signOut, refresh }),
    [session, signIn, signUp, signOut, refresh],
  );
  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const value = useContext(AuthContext);
  if (value === null) {
    throw new Error("useAuth must be used inside AuthProvider");
  }
  return value;
}
