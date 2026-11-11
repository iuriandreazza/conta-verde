import { createContext, Fragment, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { useAuth } from "../auth";
import { ptBR } from "./pt-BR";

/** The languages the screens come in. The source text of every screen is English and `pt-BR` has a catalogue that translates it (ADR 0095). */
export type Language = "pt-BR" | "en";

export const LANGUAGES: { code: Language; label: string }[] = [
  { code: "pt-BR", label: "Português" },
  { code: "en", label: "English" },
];

/** Every person who uses the app today reads Portuguese, so that is what a person who chose nothing gets. */
export const DEFAULT_LANGUAGE: Language = "pt-BR";

export type Params = Record<string, string | number>;

/** What the screens write for a text that has parts to fill in: `{name}` is replaced by the parameter of that name. */
function fill(text: string, params: Params): string {
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => (name in params ? String(params[name]) : whole));
}

/** The text of `key`, which is its English source, in `language`; a key the catalogue lacks shows as it was written. */
export function translate(language: Language, key: string, params?: Params): string {
  const text = language === "pt-BR" ? (ptBR[key] ?? key) : key;
  return params ? fill(text, params) : text;
}

/**
 * Marks a text that is written where no component can translate it (a table of labels in a module): it returns the key
 * unchanged, and the place that shows it calls `t` on it. A test reads these marks like the calls of `t`.
 */
export function msg(key: string): string {
  return key;
}

const DEVICE_KEY = "conta-verde.language";
const userKey = (userId: string) => `${DEVICE_KEY}.${userId}`;

function isLanguage(value: string | null): value is Language {
  return LANGUAGES.some((language) => language.code === value);
}

/** What was chosen on this device by this person, else by whoever last chose here; storage that is blocked is no choice. */
export function readLanguage(userId: string | null): Language {
  try {
    const own = userId === null ? null : window.localStorage.getItem(userKey(userId));
    const device = window.localStorage.getItem(DEVICE_KEY);
    return [own, device].find(isLanguage) ?? DEFAULT_LANGUAGE;
  } catch {
    return DEFAULT_LANGUAGE;
  }
}

function writeLanguage(userId: string | null, language: Language) {
  try {
    window.localStorage.setItem(DEVICE_KEY, language);
    if (userId !== null) window.localStorage.setItem(userKey(userId), language);
  } catch {
    // The choice then lasts until the page is closed, which is all a blocked storage allows.
  }
}

export type Translate = (key: string, params?: Params) => string;

/** Like `Translate`, for a sentence with a piece that is not text (a link, bold): the parameters may be nodes. */
export type Rich = (key: string, parts: Record<string, ReactNode>) => ReactNode;

interface I18nValue {
  language: Language;
  setLanguage: (language: Language) => void;
  t: Translate;
  rich: Rich;
}

function build(language: Language, setLanguage: (language: Language) => void): I18nValue {
  const t: Translate = (key, params) => translate(language, key, params);
  const rich: Rich = (key, parts) => {
    // Split around each `{name}` and put the node in its place; text between stays text.
    const pieces = translate(language, key).split(/(\{\w+\})/);
    return pieces.map((piece, index) => {
      const name = /^\{(\w+)\}$/.exec(piece)?.[1];
      return <Fragment key={index}>{name !== undefined && name in parts ? parts[name] : piece}</Fragment>;
    });
  };
  return { language, setLanguage, t, rich };
}

// Outside a provider (a test of one component) the default language is used and choosing does nothing.
const I18nContext = createContext<I18nValue>(build(DEFAULT_LANGUAGE, () => undefined));

/**
 * Gives the screens their language. It lives inside the session, because the choice is remembered **per person** on the
 * device: signing in as someone else reads theirs, and a person who never chose gets the language last chosen here.
 */
export function I18nProvider({ children }: { children: ReactNode }) {
  const { session } = useAuth();
  const userId = session.status === "authenticated" ? session.profile.user.id : null;
  const [language, setChosen] = useState<Language>(() => readLanguage(null));
  useEffect(() => setChosen(readLanguage(userId)), [userId]);
  useEffect(() => {
    document.documentElement.lang = language;
  }, [language]);
  const setLanguage = useCallback(
    (next: Language) => {
      setChosen(next);
      writeLanguage(userId, next);
    },
    [userId],
  );
  const value = useMemo(() => build(language, setLanguage), [language, setLanguage]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  return useContext(I18nContext);
}

/** The translation function of the current language: `const t = useT(); t("Sign in")`. */
export function useT(): Translate {
  return useI18n().t;
}
