import { motion } from "motion/react";
import { useState } from "react";
import { Link, Outlet, useLocation } from "react-router-dom";
import { useAuth } from "../auth";
import { Button } from "./Button";
import { Footer } from "./Footer";
import { LogoutIcon } from "./icons/logout";
import { Logo } from "./Logo";

export function Layout() {
  const { session, signOut } = useAuth();
  const [signOutFailed, setSignOutFailed] = useState(false);
  const [signingOut, setSigningOut] = useState(false);
  const location = useLocation();

  // The session only ends once the server confirms it, so a failure must stay visible.
  function handleSignOut() {
    setSignOutFailed(false);
    setSigningOut(true);
    signOut()
      .catch(() => setSignOutFailed(true))
      .finally(() => setSigningOut(false));
  }

  return (
    <div className="shell">
      <header className="topbar">
        <Link to="/" className="brand" aria-label="conta-verde, início">
          <Logo size={30} tile={false} />
        </Link>
        {session.status === "authenticated" && (
          <div className="topbar-user">
            {signOutFailed && <span role="alert">Não foi possível sair, tente novamente.</span>}
            <span className="avatar" aria-hidden>
              {session.profile.user.name.trim().charAt(0).toUpperCase()}
            </span>
            <span className="user-name">{session.profile.user.name}</span>
            <Button variant="ghost" icon={LogoutIcon} pending={signingOut} onClick={handleSignOut}>
              Sair
            </Button>
          </div>
        )}
      </header>
      {/* Enter-only on purpose. An exit phase (AnimatePresence "wait") left the page stuck invisible
          when redirects chained, e.g. sign-up -> "/" -> company page. Remounting per path is robust. */}
      <motion.main
        key={location.pathname}
        initial={{ opacity: 0, y: 10 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.2, ease: "easeOut" }}
      >
        <Outlet />
      </motion.main>
      <Footer />
    </div>
  );
}
