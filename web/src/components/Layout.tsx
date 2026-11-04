import { Link, Outlet } from "react-router-dom";
import { useAuth } from "../auth";
import { Footer } from "./Footer";

export function Layout() {
  const { session, signOut } = useAuth();

  return (
    <div className="shell">
      <header className="topbar">
        <Link to="/" className="brand">
          conta-verde
        </Link>
        {session.status === "authenticated" && (
          <div className="topbar-user">
            <span>{session.profile.user.name}</span>
            <button type="button" onClick={() => void signOut()}>
              Sign out
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
