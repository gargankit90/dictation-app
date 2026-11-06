import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles/tokens.css";
import { Overlay } from "./Overlay";
import "./overlay-window.css";

const root = document.getElementById("root");
if (!root) {
  throw new Error("Missing #root element in overlay.html");
}

createRoot(root).render(
  <StrictMode>
    <Overlay />
  </StrictMode>,
);
