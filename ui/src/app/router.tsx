// Route table. Routes are code-split via React Router's `lazy` (each screen
// module exports `Component`), so the initial chunk stays small (UI_REFERENCE §8;
// react-markdown only loads with /recall). Every route declares an errorElement so
// a thrown render is caught per-route, not app-wide (UI_REFERENCE §6).
import { createBrowserRouter } from "react-router-dom";

import { AppShell } from "../components/shell/AppShell";
import { RouteError } from "../routes/RouteError";

export const router = createBrowserRouter([
  {
    path: "/",
    element: <AppShell />,
    errorElement: <RouteError />,
    children: [
      {
        index: true,
        lazy: () => import("../routes/Deck"),
        errorElement: <RouteError />,
      },
      {
        path: "recall",
        lazy: () => import("../routes/Recall"),
        errorElement: <RouteError />,
      },
      {
        path: "timeline",
        lazy: () => import("../routes/Timeline"),
        errorElement: <RouteError />,
      },
      {
        path: "timeline/session/:id",
        lazy: () => import("../routes/Session"),
        errorElement: <RouteError />,
      },
      {
        path: "timeline/:id",
        lazy: () => import("../routes/Moment"),
        errorElement: <RouteError />,
      },
      {
        path: "insights",
        lazy: () => import("../routes/Insights"),
        errorElement: <RouteError />,
      },
      {
        path: "settings",
        lazy: () => import("../routes/Settings"),
        errorElement: <RouteError />,
      },
      { path: "*", lazy: () => import("../routes/NotFound") },
    ],
  },
  // Dev-only design-system specimen. It sits OUTSIDE the AppShell on purpose:
  // /kitchen renders the v2 primitives and tokens alone, so a P2 review measures
  // the design system and not the v1 shell that P7 replaces. Tree-shaken in prod.
  ...(import.meta.env.DEV
    ? [{ path: "/kitchen", lazy: () => import("../routes/Kitchen"), errorElement: <RouteError /> }]
    : []),
]);
