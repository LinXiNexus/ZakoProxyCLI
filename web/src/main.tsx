import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./style.css";
import { loadPrefs } from "./prefs";

function applyUiPrefs() {
  const prefs = loadPrefs();
  document.body.setAttribute("data-font", prefs.fontFamily);
  document.body.setAttribute("data-bg", prefs.uiBackground);
}
applyUiPrefs();
window.addEventListener("zako-prefs", applyUiPrefs);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
