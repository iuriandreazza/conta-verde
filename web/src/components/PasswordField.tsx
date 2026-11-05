import { useId, useState } from "react";
import { EyeIcon } from "./icons/eye";
import { EyeOffIcon } from "./icons/eye-off";

interface PasswordFieldProps {
  registering: boolean;
}

export function PasswordField({ registering }: PasswordFieldProps) {
  const [visible, setVisible] = useState(false);
  const inputId = useId();
  const Icon = visible ? EyeOffIcon : EyeIcon;

  // The toggle sits outside the <label> so it does not become part of the input's accessible name.
  return (
    <div className="field">
      <label htmlFor={inputId}>Senha</label>
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
          aria-label={visible ? "Ocultar senha" : "Mostrar senha"}
          aria-pressed={visible}
          onClick={() => setVisible((current) => !current)}
        >
          <Icon size={18} aria-hidden />
        </button>
      </span>
    </div>
  );
}
