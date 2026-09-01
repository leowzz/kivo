import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import ProductStudioApp from "./ProductStudioApp";
import "../styles/tokens.css";
import "../styles/base.css";
import "../styles/app.css";
import "../styles/views.css";
import "./studio.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ProductStudioApp />
  </StrictMode>,
);
