// Route table. Routes are code-split via React Router's `lazy` (each screen
// module exports `Component`), so the initial chunk stays small (UI_REFERENCE §8;
// react-markdown only loads with /recall). Every route declares an errorElement so
// a thrown render is caught per-route, not app-wide (UI_REFERENCE §6).
//
// `/settings` and `/settings/:category` load the same module: Settings v2 is one
// shell with a category sidebar (`routes/settings/index.tsx`), and an unknown or
// absent category falls back to General there, so a stale deep link never
// dead-ends. The old single-file `routes/Settings.tsx` is gone; both paths point
// at the lowercase folder, which is also why the case-only module ambiguity
// (TS1261) is resolved here.
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
        lazy: () => import("../routes/settings"),
        errorElement: <RouteError />,
      },
      {
        path: "settings/:category",
        lazy: () => import("../routes/settings"),
        errorElement: <RouteError />,
      },
      { path: "*", lazy: () => import("../routes/NotFound") },
    ],
  },
  // Dev-only design-system specimen. It sits OUTSIDE the AppShell on purpose:
  // /kitchen renders the v2 primitives and tokens alone, so a P2 review measures
  // the design system and not the app shell. Tree-shaken in prod.
  ...(import.meta.env.DEV
    ? [{ path: "/kitchen", lazy: () => import("../routes/Kitchen"), errorElement: <RouteError /> }]
    : []),
]);
