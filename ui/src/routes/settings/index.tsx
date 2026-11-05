// Route entry for /settings and /settings/:category. An unknown or absent
// category falls back to General, so a stale deep link never dead-ends.
import { useParams } from "react-router-dom";

import { SettingsLayout } from "./SettingsLayout";
import { isCategoryId } from "./registry";

export function Component() {
  const { category } = useParams();
  return (
    <SettingsLayout category={isCategoryId(category) ? category : "general"} />
  );
}
