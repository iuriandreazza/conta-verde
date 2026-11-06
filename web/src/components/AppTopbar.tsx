import { Button } from "./Button";
import { BoxIcon } from "./icons/box";
import { CircleDollarSignIcon } from "./icons/circle-dollar-sign";
import { FileTextIcon } from "./icons/file-text";
import { HandCoinsIcon } from "./icons/hand-coins";
import { LogoutIcon } from "./icons/logout";
import { PlusIcon } from "./icons/plus";
import { SendIcon } from "./icons/send";
import { UsersIcon } from "./icons/users";
import { MenuButton, type MenuItem } from "./MenuButton";
import type { Area, Company } from "../api";
import { useCreation, type CreateKind } from "../creation/CreationProvider";

interface AppTopbarProps {
  userName: string;
  permissions: Company["permissions"] | undefined;
  companyId: string | undefined;
  signingOut: boolean;
  signOutFailed: boolean;
  navOpen: boolean;
  onToggleNav: () => void;
  onSignOut: () => void;
}

/** What a person can start from anywhere: only the registers their role may write. */
function creationItems(
  companyId: string,
  permissions: AppTopbarProps["permissions"],
  openCreate: (kind: CreateKind) => void,
): MenuItem[] {
  const base = `/companies/${companyId}`;
  const items: (MenuItem & { area: Area; creates?: CreateKind })[] = [
    { key: "entry", label: "Novo lançamento", icon: CircleDollarSignIcon, creates: "entry", area: "finance" },
    { key: "transfer", label: "Nova transferência", icon: SendIcon, creates: "transfer", area: "finance" },
    { key: "quote", label: "Novo orçamento", icon: FileTextIcon, to: `${base}/quotes`, area: "sales" },
    { key: "sale", label: "Nova venda", icon: HandCoinsIcon, to: `${base}/sales`, area: "sales" },
    { key: "party", label: "Novo cliente ou fornecedor", icon: UsersIcon, creates: "party", area: "parties" },
    { key: "product", label: "Novo produto ou serviço", icon: BoxIcon, creates: "product", area: "products" },
  ];
  return items
    .filter((item) => permissions?.[item.area] === "write")
    // A register with a sheet opens it over the page that is showing; the others still go to their page.
    .map(({ creates, area: _area, ...item }) => (creates ? { ...item, onSelect: () => openCreate(creates) } : item));
}

/** The bar over every signed-in page: the menu on a phone, "new" from anywhere, who is signed in and how to leave. */
export function AppTopbar({
  userName,
  permissions,
  companyId,
  signingOut,
  signOutFailed,
  navOpen,
  onToggleNav,
  onSignOut,
}: AppTopbarProps) {
  const { openCreate } = useCreation();
  const creating = companyId ? creationItems(companyId, permissions, openCreate) : [];
  return (
    <div className="topbar">
      {companyId && (
        <button
          type="button"
          className="topbar-menu"
          aria-expanded={navOpen}
          aria-controls="app-sidebar"
          onClick={onToggleNav}
        >
          Menu
        </button>
      )}
      <div className="topbar-actions">
        {creating.length > 0 && <MenuButton variant="primary" icon={PlusIcon} label="Novo registro" items={creating} align="end" />}
        <span className="avatar" aria-hidden>
          {userName.trim().charAt(0).toUpperCase()}
        </span>
        <span className="user-name">{userName}</span>
        <Button variant="ghost" icon={LogoutIcon} pending={signingOut} onClick={onSignOut}>
          Sair
        </Button>
        {signOutFailed && <span role="alert">Não foi possível sair, tente novamente.</span>}
      </div>
    </div>
  );
}
