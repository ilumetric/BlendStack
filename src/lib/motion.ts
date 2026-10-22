import type { CSSProperties } from "react";

/** Index for staggered entrance animations (`.enter` in styles.css). */
export function stagger(i: number): CSSProperties {
  return { "--i": Math.min(i, 14) } as CSSProperties;
}
