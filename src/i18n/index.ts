import { useStore } from "../store";
import { translate, type MessageKey, type Translate } from "./core";

export { LANGUAGES, languageDirection, resolveLanguage, translate } from "./core";
export type { MessageKey, Translate } from "./core";

export function useT(): Translate {
  const lang = useStore((s) => s.lang);
  return (key, vars) => translate(lang, key, vars);
}

/** For use outside React components (toasts, callbacks). */
export function t(key: MessageKey, vars?: Record<string, string | number>): string {
  return translate(useStore.getState().lang, key, vars);
}
