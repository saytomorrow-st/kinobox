import { getCurrentWindow } from "@tauri-apps/api/window";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import "./index.css";

const label = getCurrentWindow().label;
const transparentWindows = ["login", "player-controls"];
document.body.classList.add(transparentWindows.includes(label) ? "bg-transparent" : "bg-bg");

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(<App label={label} />);
}
