import { useId, useState } from "react";
import { useT } from "../i18n";
import { EyeIcon } from "./icons/eye";
import { EyeOffIcon } from "./icons/eye-off";

interface PasswordFieldProps {
  /** A password being chosen (a sign-up, a reset) rather than typed in: it has a minimum and is not filled in. */
  registering: boolean;
  label?: string;
}

export function PasswordField({ registering, label }: PasswordFieldProps) {
  const t = useT();
  const [visible, setVisible] = useState(false);
  const inputId = useId();
  const Icon = visible ? EyeOffIcon : EyeIcon;

  // The toggle sits outside the <label> so it does not become part of the input's accessible name.
  return (
    <div className="field">
      <label htmlFor={inputId}>{label ?? t("Password")}</label>
      <span className="field-with-action">
        <input
          id={inputId}
          name="password"
          type={visible ? "text" : "password"}
          required
          minLength={registering ? 8 : undefined}
          autoComplete={registering ? "new-password" : "current-password"}
        />
        <button
          type="button"
          className="field-action"
          aria-label={visible ? t("Hide password") : t("Show password")}
          aria-pressed={visible}
          onClick={() => setVisible((current) => !current)}
        >
          <Icon size={18} aria-hidden />
        </button>
      </span>
    </div>
  );
}
