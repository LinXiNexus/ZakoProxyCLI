import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./style.css";
import { loadPrefs } from "./prefs";

function applyUiPrefs() {
  const prefs = loadPrefs();
  const body = document.body;
  body.setAttribute("data-font", prefs.fontFamily);
  body.setAttribute("data-bg", prefs.bgImage && prefs.uiBackground === "image" ? "image" : prefs.uiBackground);
  if (prefs.bgImage) {
    document.documentElement.style.setProperty("--bg-image", `url("${prefs.bgImage}")`);
  } else {
    document.documentElement.style.removeProperty("--bg-image");
  }
  const dark =
    prefs.uiTheme === "dark" ||
    (prefs.uiTheme === "auto" &&
      window.matchMedia("(prefers-color-scheme: dark)").matches);
  body.setAttribute("data-mode", dark ? "dark" : "light");
}
applyUiPrefs();
window.addEventListener("zako-prefs", applyUiPrefs);
window
  .matchMedia("(prefers-color-scheme: dark)")
  .addEventListener("change", applyUiPrefs);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
