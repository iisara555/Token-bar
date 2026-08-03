import React from "react";
import ReactDOM from "react-dom/client";
import "../styles/tokens.css";
import "../styles/app.css";
import { Settings } from "../views/Settings";
import { bootTheme } from "./boot";

bootTheme();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Settings />
  </React.StrictMode>,
);
