import { LANGUAGES, useI18n, type Language } from "../i18n";

/** Chooses the language of the screens; it is remembered for the person on this device. */
export function LanguageSwitch() {
  const { language, setLanguage, t } = useI18n();
  return (
    <select
      className="language-switch"
      aria-label={t("Language")}
      value={language}
      onChange={(event) => setLanguage(event.target.value as Language)}
    >
      {LANGUAGES.map((option) => (
        <option key={option.code} value={option.code}>
          {option.label}
        </option>
      ))}
    </select>
  );
}
