import React from "react";
import ReactDOM from "react-dom/client";
import "../styles/tokens.css";
import "../styles/app.css";
import { Popover } from "../views/Popover";
import { bootTheme } from "./boot";

bootTheme();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Popover />
  </React.StrictMode>,
);
