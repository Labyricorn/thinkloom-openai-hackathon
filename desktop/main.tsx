import React from "react";
import { createRoot } from "react-dom/client";
import Thinkloom from "../app/thinkloom";
import "../app/globals.css";

createRoot(document.getElementById("root")!).render(<React.StrictMode><Thinkloom /></React.StrictMode>);
