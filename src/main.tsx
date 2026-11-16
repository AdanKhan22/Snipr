import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import App from "./App";
import Picker from "./Picker";
import Prompt from "./Prompt";
import "./styles.css";

const label = getCurrentWebviewWindow().label;
document.documentElement.dataset.window = label;

const Root = label === "picker" ? Picker : label === "prompt" ? Prompt : App;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
