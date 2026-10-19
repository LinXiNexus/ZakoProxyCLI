export type Prefs = {
  fontSize: "small" | "normal" | "large";
  messageStyle: "plain" | "bubble";
  fontFamily: "default" | "system" | "serif" | "mono";
  uiBackground: "aurora" | "plain" | "sunset" | "ocean";
  thinkingOpen: boolean;
  showTools: boolean;
  showTime: boolean;
  showModelLabel: boolean;
  wrapCode: boolean;
  enterToSend: boolean;
  defaultRoot: boolean;
  compact: boolean;
};

export const defaultPrefs: Prefs = {
  fontSize: "normal",
  messageStyle: "plain",
  fontFamily: "default",
  uiBackground: "aurora",
  thinkingOpen: false,
  showTools: true,
  showTime: false,
  showModelLabel: true,
  wrapCode: true,
  enterToSend: true,
  defaultRoot: true,
  compact: false,
};

const KEY = "zako.prefs.v1";

export function loadPrefs(): Prefs {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...defaultPrefs };
    return { ...defaultPrefs, ...JSON.parse(raw) };
  } catch {
    return { ...defaultPrefs };
  }
}

export function savePrefs(prefs: Prefs) {
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    /* ignore */
  }
  window.dispatchEvent(new Event("zako-prefs"));
}
