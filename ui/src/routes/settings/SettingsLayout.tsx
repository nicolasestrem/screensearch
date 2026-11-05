// Settings v2 layout (UI_REFERENCE §3 §8): category sidebar, one search field,
// one pane that renders the active category. The sidebar is sticky inside the
// shell's single scroll pane and never scrolls on its own.
import { useEffect, useState, type ComponentType } from "react";
import { NavLink, useNavigate } from "react-router-dom";

import { cn } from "../../lib/cn";
import { AdvancedCategory } from "./Advanced";
import { CaptureCategory } from "./Capture";
import { DataCategory } from "./Data";
import { GeneralCategory } from "./General";
import { IntegrationsCategory } from "./Integrations";
import { IntelligenceCategory } from "./Intelligence";
import { ModelsCategory } from "./Models";
import { PrivacyCategory } from "./Privacy";
import {
  SETTING_CATEGORIES,
  settingAnchorId,
  type SettingCategoryId,
  type SettingKey,
} from "./registry";
import { SettingsSearch } from "./SettingsSearch";

const CATEGORY_COMPONENTS: Record<SettingCategoryId, ComponentType> = {
  general: GeneralCategory,
  capture: CaptureCategory,
  privacy: PrivacyCategory,
  intelligence: IntelligenceCategory,
  models: ModelsCategory,
  data: DataCategory,
  integrations: IntegrationsCategory,
  advanced: AdvancedCategory,
};

const CATEGORY_LINK = cn(
  "min-w-0 rounded-md px-3 py-2 text-t-ui",
  "transition-colors duration-fast ease-standard",
  "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus",
);

export interface SettingsLayoutProps {
  category: SettingCategoryId;
}

export function SettingsLayout({ category }: SettingsLayoutProps) {
  const navigate = useNavigate();
  const [pendingAnchor, setPendingAnchor] = useState<string | null>(null);
  const active =
    SETTING_CATEGORIES.find((entry) => entry.id === category) ??
    SETTING_CATEGORIES[0];
  const Category = CATEGORY_COMPONENTS[category];

  // A search result must land on its row even when the row's data is still
  // arriving: retry for a few frames, then give up quietly rather than keeping a
  // stale focus request alive.
  useEffect(() => {
    if (!pendingAnchor) return;
    let frame = 0;
    let attempts = 0;
    const focusAnchor = () => {
      const element = document.getElementById(pendingAnchor);
      if (element) {
        element.scrollIntoView({ block: "center" });
        element.focus({ preventScroll: true });
        setPendingAnchor(null);
        return;
      }
      if (attempts < 30) {
        attempts += 1;
        frame = requestAnimationFrame(focusAnchor);
      } else {
        setPendingAnchor(null);
      }
    };
    frame = requestAnimationFrame(focusAnchor);
    return () => cancelAnimationFrame(frame);
  }, [pendingAnchor, category]);

  return (
    <div className="mx-auto flex min-w-0 max-w-5xl flex-col gap-pane px-pane py-pane">
      <header className="min-w-0">
        <h1 className="text-t-h1 font-semibold text-fg-primary">Settings</h1>
        <p className="mt-1 max-w-prose text-t-ui text-fg-secondary">
          Change one behaviour at a time;
          every row saves on its own and offers an undo.
        </p>
      </header>

      <SettingsSearch
        onNavigate={(target, key: SettingKey) => {
          if (target !== category) navigate(`/settings/${target}`);
          setPendingAnchor(settingAnchorId(key));
        }}
      />

      <div className="grid min-w-0 grid-cols-12 gap-gap">
        <aside className="col-span-12 min-w-0 md:col-span-3">
          <nav
            aria-label="Settings categories"
            className="sticky top-0 z-rail flex min-w-0 flex-col gap-1 bg-surface-app py-1"
          >
            {SETTING_CATEGORIES.map((entry) => (
              <NavLink
                key={entry.id}
                to={`/settings/${entry.id}`}
                className={({ isActive }) =>
                  cn(
                    CATEGORY_LINK,
                    isActive
                      ? "bg-surface-selected font-medium text-fg-primary"
                      : "text-fg-secondary hover:bg-surface-hover hover:text-fg-primary",
                  )
                }
              >
                {entry.label}
              </NavLink>
            ))}
          </nav>
        </aside>

        <div className="col-span-12 min-w-0 md:col-span-9">
          <div className="mb-2 min-w-0">
            <h2 className="text-t-h3 font-medium text-fg-primary">{active.label}</h2>
            <p className="mt-1 max-w-prose text-t-sm text-fg-secondary">
              {active.summary}
            </p>
          </div>
          <Category />
        </div>
      </div>
    </div>
  );
}
