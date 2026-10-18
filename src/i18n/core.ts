import { en, type MessageKey } from "./en";
import { ru } from "./ru";

const dictionaries: Record<string, Record<MessageKey, string>> = { en, ru };

export const LANGUAGES = [
  { code: "en", name: "English" },
  { code: "ru", name: "Русский" },
];

export function resolveLanguage(preferred: string | undefined): string {
  const code = (preferred || navigator.language || "en").slice(0, 2).toLowerCase();
  return code in dictionaries ? code : "en";
}

export function translate(lang: string, key: MessageKey, vars?: Record<string, string | number>): string {
  let text = dictionaries[lang]?.[key] ?? en[key] ?? key;
  if (vars) for (const [k, v] of Object.entries(vars)) text = text.split(`{${k}}`).join(String(v));
  return text;
}

export type Translate = (key: MessageKey, vars?: Record<string, string | number>) => string;
export type { MessageKey };
