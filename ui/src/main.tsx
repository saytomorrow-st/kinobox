import { getCurrentWindow } from "@tauri-apps/api/window";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import "./index.css";

const label = getCurrentWindow().label;
document.body.classList.add(label === "login" ? "bg-transparent" : "bg-bg");

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(<App label={label} />);
}
