import React from "react";
import { createRoot } from "react-dom/client";
import Thinkloom from "./Thinkloom";
import "./globals.css";

createRoot(document.getElementById("root")!).render(
  <React.StrictMode><Thinkloom /></React.StrictMode>,
);
