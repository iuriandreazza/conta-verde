import { DEFAULT_LANGUAGE, translate, type Translate } from "../i18n";

/** For a helper that may be called where no component can give the language of the screen (and by tests): it speaks the default one. */
export const inDefaultLanguage: Translate = (key, params) => translate(DEFAULT_LANGUAGE, key, params);
