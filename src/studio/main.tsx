import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import StudioApp from "./StudioApp";
import "../styles/tokens.css";
import "../styles/base.css";
import "./studio.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <StudioApp />
  </StrictMode>,
);
