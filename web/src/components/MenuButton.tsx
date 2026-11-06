import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { Button, type AnimatedIcon } from "./Button";
import { ChevronDownIcon } from "./icons/chevron-down";

export interface MenuItem {
  key: string;
  label: string;
  icon?: AnimatedIcon;
  /** Goes to another page, as a link. */
  to?: string;
  /** Does something here, as a button. */
  onSelect?: () => void;
  disabled?: boolean;
}

interface MenuButtonProps {
  label: ReactNode;
  items: MenuItem[];
  icon?: AnimatedIcon;
  variant?: "primary" | "ghost";
  /** Which edge of the button the menu lines up with. */
  align?: "start" | "end";
}

/**
 * A button that opens a list of choices: arrows move, Home and End jump, Escape closes and gives the
 * focus back to the button, and so does choosing one. One menu is open at a time because a click
 * anywhere else closes it.
 */
export function MenuButton({ label, items, icon, variant = "ghost", align = "start" }: MenuButtonProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const menuId = useId();

  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);

  useEffect(() => {
    if (open) enabledItems()[0]?.focus();
  }, [open]);

  function enabledItems(): HTMLElement[] {
    return Array.from(menuRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]:not([aria-disabled="true"])') ?? []);
  }

  function closeAndRefocus() {
    setOpen(false);
    triggerRef.current?.focus();
  }

  function moveFocus(event: KeyboardEvent) {
    const entries = enabledItems();
    const at = entries.indexOf(document.activeElement as HTMLElement);
    const target =
      event.key === "ArrowDown"
        ? entries[(at + 1) % entries.length]
        : event.key === "ArrowUp"
          ? entries[(at - 1 + entries.length) % entries.length]
          : event.key === "Home"
            ? entries[0]
            : event.key === "End"
              ? entries[entries.length - 1]
              : undefined;
    if (target) {
      event.preventDefault();
      target.focus();
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeAndRefocus();
    } else if (event.key === "Tab") {
      setOpen(false);
    }
  }

  return (
    <div className="menu-button" ref={rootRef}>
      <Button
        ref={triggerRef}
        type="button"
        variant={variant}
        icon={icon}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={() => setOpen(!open)}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" && !open) {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        <span className="menu-label">
          {label}
          <ChevronDownIcon size={14} aria-hidden />
        </span>
      </Button>
      {open && (
        <div className={`menu menu-${align}`} role="menu" id={menuId} ref={menuRef} onKeyDown={moveFocus}>
          {items.map((item) => {
            const content = (
              <>
                {item.icon && <item.icon size={16} aria-hidden />}
                {item.label}
              </>
            );
            return item.to !== undefined && !item.disabled ? (
              <Link key={item.key} role="menuitem" to={item.to} onClick={() => setOpen(false)}>
                {content}
              </Link>
            ) : (
              <button
                key={item.key}
                type="button"
                role="menuitem"
                aria-disabled={item.disabled || undefined}
                onClick={() => {
                  if (item.disabled) return;
                  closeAndRefocus();
                  item.onSelect?.();
                }}
              >
                {content}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
