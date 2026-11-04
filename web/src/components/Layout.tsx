import { useState } from "react";
import { Link, Outlet } from "react-router-dom";
import { useAuth } from "../auth";
import { Footer } from "./Footer";

export function Layout() {
  const { session, signOut } = useAuth();
  const [signOutFailed, setSignOutFailed] = useState(false);

  // The session only ends once the server confirms it, so a failure must stay visible.
  function handleSignOut() {
    setSignOutFailed(false);
    signOut().catch(() => setSignOutFailed(true));
  }

  return (
    <div className="shell">
      <header className="topbar">
        <Link to="/" className="brand">
          conta-verde
        </Link>
        {session.status === "authenticated" && (
          <div className="topbar-user">
            {signOutFailed && <span role="alert">Não foi possível sair, tente novamente.</span>}
            <span>{session.profile.user.name}</span>
            <button type="button" onClick={handleSignOut}>
              Sair
            </button>
          </div>
        )}
      </header>
      <main>
        <Outlet />
      </main>
      <Footer />
    </div>
  );
}
