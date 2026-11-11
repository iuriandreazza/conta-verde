import { admin } from "./admin";
import { auth } from "./auth";
import { sales } from "./sales";
import { shell } from "./shell";

/**
 * The Portuguese catalogue: the English text a screen is written in, and what it says in Portuguese. One file for each area of
 * the app, so that the pull requests that translate the areas do not meet in one file; a test refuses a key that two files give.
 */
export const catalogues: Record<string, Record<string, string>> = { admin, auth, sales, shell };

export const ptBR: Record<string, string> = Object.assign({}, ...Object.values(catalogues));
