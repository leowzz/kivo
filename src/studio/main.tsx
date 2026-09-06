import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import StudioRoot from "./StudioRoot";
import "../shared/styles/tokens.css";
import "../shared/styles/base.css";
import "./studio.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <StudioRoot />
  </StrictMode>,
);
