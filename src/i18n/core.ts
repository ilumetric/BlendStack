import { en, type MessageKey } from "./en";
import { ru } from "./ru";
import { de } from "./de";
import { fr } from "./fr";
import { es } from "./es";
import { it } from "./it";
import { pt } from "./pt";
import { pl } from "./pl";
import { uk } from "./uk";
import { tr } from "./tr";
import { ja } from "./ja";
import { ko } from "./ko";
import { zh } from "./zh";

const dictionaries: Record<string, Record<MessageKey, string>> = { en, ru, de, fr, es, it, pt, pl, uk, tr, ja, ko, zh };

export const LANGUAGES = [
  { code: "en", name: "English" },
  { code: "ru", name: "Русский" },
  { code: "de", name: "Deutsch" },
  { code: "fr", name: "Français" },
  { code: "es", name: "Español" },
  { code: "it", name: "Italiano" },
  { code: "pt", name: "Português" },
  { code: "pl", name: "Polski" },
  { code: "uk", name: "Українська" },
  { code: "tr", name: "Türkçe" },
  { code: "ja", name: "日本語" },
  { code: "ko", name: "한국어" },
  { code: "zh", name: "简体中文" },
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
