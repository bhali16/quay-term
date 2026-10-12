import type { ITheme } from "@xterm/xterm";

/** Chrome colors become CSS variables on :root; `term` goes to xterm. */
export type Theme = {
  id: string;
  name: string;
  dark: boolean;
  ui: { bg: string; bgAlt: string; bgDeep: string; surface: string; hover: string; border: string; input: string; fg: string; accent: string; dim: string; green: string; red: string; yellow: string };
  term: ITheme;
};

const ansi = (c: string[]): Partial<ITheme> => ({
  black: c[0], red: c[1], green: c[2], yellow: c[3], blue: c[4], magenta: c[5], cyan: c[6], white: c[7],
  brightBlack: c[8], brightRed: c[9], brightGreen: c[10], brightYellow: c[11], brightBlue: c[12], brightMagenta: c[13], brightCyan: c[14], brightWhite: c[15],
});

export const themes: Theme[] = [
  {
    id: "quay-dark",
    name: "Quay Dark",
    dark: true,
    ui: { bg: "#1e1f24", bgAlt: "#202127", bgDeep: "#16171b", surface: "#2b2d36", hover: "#33353e", border: "#44464f", input: "#111111", fg: "#e6e6e6", accent: "#3b6fd8", dim: "#9297a6", green: "#4ec9a0", red: "#f06b6b", yellow: "#e5c07b" },
    term: {
      background: "#1e1f24", foreground: "#e6e6e6", cursor: "#e6e6e6", selectionBackground: "#3b6fd855",
      ...ansi(["#2b2d36", "#f06b6b", "#4ec9a0", "#e5c07b", "#5b8cff", "#c678dd", "#56b6c2", "#c9ccd6", "#5c6070", "#ff8585", "#6fe0b8", "#f0d195", "#7aa2ff", "#d896ec", "#78cfd9", "#ffffff"]),
    },
  },
  {
    id: "quay-light",
    name: "Quay Light",
    dark: false,
    ui: { bg: "#fafafa", bgAlt: "#ececf0", bgDeep: "#dcdce2", surface: "#ffffff", hover: "#e2e2e8", border: "#c4c4cc", input: "#ffffff", fg: "#1f2328", accent: "#2f5fc4", dim: "#566069", green: "#1a7f37", red: "#cf222e", yellow: "#9a6700" },
    term: {
      background: "#fafafa", foreground: "#1f2328", cursor: "#1f2328", selectionBackground: "#2f5fc433",
      ...ansi(["#24292f", "#cf222e", "#116329", "#9a6700", "#0969da", "#8250df", "#1b7c83", "#6e7781", "#57606a", "#a40e26", "#1a7f37", "#7d4e00", "#218bff", "#a475f9", "#3192aa", "#8c959f"]),
    },
  },
  {
    id: "dracula",
    name: "Dracula",
    dark: true,
    ui: { bg: "#282a36", bgAlt: "#21222c", bgDeep: "#191a21", surface: "#343746", hover: "#3d4052", border: "#555a75", input: "#191a21", fg: "#f8f8f2", accent: "#bd93f9", dim: "#9aa3c7", green: "#50fa7b", red: "#ff5555", yellow: "#f1fa8c" },
    term: {
      background: "#282a36", foreground: "#f8f8f2", cursor: "#f8f8f2", selectionBackground: "#44475a",
      ...ansi(["#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2", "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df", "#a4ffff", "#ffffff"]),
    },
  },
  {
    id: "solarized-dark",
    name: "Solarized Dark",
    dark: true,
    ui: { bg: "#002b36", bgAlt: "#073642", bgDeep: "#00212b", surface: "#0b3a47", hover: "#124755", border: "#2c5b68", input: "#00212b", fg: "#93a1a1", accent: "#268bd2", dim: "#8fa5aa", green: "#859900", red: "#dc322f", yellow: "#b58900" },
    term: {
      background: "#002b36", foreground: "#93a1a1", cursor: "#93a1a1", selectionBackground: "#073642",
      ...ansi(["#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5", "#586e75", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3"]),
    },
  },
  {
    id: "solarized-light",
    name: "Solarized Light",
    dark: false,
    ui: { bg: "#fdf6e3", bgAlt: "#eee8d5", bgDeep: "#e4ddc6", surface: "#fffaf0", hover: "#e8e1ca", border: "#c9c2a8", input: "#fffaf0", fg: "#586e75", accent: "#268bd2", dim: "#566b72", green: "#5f7a00", red: "#c02a27", yellow: "#8a6700" },
    term: {
      background: "#fdf6e3", foreground: "#586e75", cursor: "#586e75", selectionBackground: "#eee8d5",
      ...ansi(["#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198", "#eee8d5", "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4", "#93a1a1", "#fdf6e3"]),
    },
  },
];

export const themeById = (id: string) => themes.find((t) => t.id === id) ?? themes[0];

/** Black or white, whichever reads better on `hex` (WCAG relative luminance). */
function readableOn(hex: string) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.3 ? "#16171b" : "#ffffff";
}

export function applyChrome(t: Theme) {
  const r = document.documentElement.style;
  const m: Record<string, string> = {
    bg: t.ui.bg, "bg-alt": t.ui.bgAlt, "bg-deep": t.ui.bgDeep, surface: t.ui.surface, hover: t.ui.hover,
    border: t.ui.border, input: t.ui.input, fg: t.ui.fg, accent: t.ui.accent,
    dim: t.ui.dim, green: t.ui.green, red: t.ui.red, yellow: t.ui.yellow,
  };
  for (const [k, v] of Object.entries(m)) r.setProperty(`--${k}`, v);
  r.setProperty("--on-accent", readableOn(t.ui.accent));
  r.colorScheme = t.dark ? "dark" : "light";
}
