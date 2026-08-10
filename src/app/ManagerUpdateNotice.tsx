import { useEffect, useId, useState } from "react";

import {
  MANAGER_UPDATE_GUIDE_URL,
  managerApi,
  type ManagerUpdateAvailable,
} from "../services/managerApi";
import { Ring } from "./components";
import { useI18n } from "./i18n";
import { Sheet } from "./Sheet";

const LAST_CHECK_KEY = "cam.manager-update.last-check";
export const MANAGER_UPDATE_CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

export function managerUpdateCheckDue(lastCheck: string | null, now = Date.now()): boolean {
  if (!lastCheck) return true;
  const checkedAt = Number(lastCheck);
  return (
    !Number.isFinite(checkedAt) ||
    checkedAt < 0 ||
    checkedAt > now ||
    now - checkedAt >= MANAGER_UPDATE_CHECK_INTERVAL_MS
  );
}

/** Daily metadata-only check for a newer full offline installer. */
export function ManagerUpdateNotice() {
  const { t } = useI18n();
  const [update, setUpdate] = useState<ManagerUpdateAvailable | null>(null);
  const titleId = useId();
  const bodyId = useId();

  useEffect(() => {
    const now = Date.now();
    let lastCheck: string | null = null;
    try {
      lastCheck = localStorage.getItem(LAST_CHECK_KEY);
    } catch {
      // A denied storage API should not disable the lightweight update check.
    }
    if (!managerUpdateCheckDue(lastCheck, now)) return;

    try {
      localStorage.setItem(LAST_CHECK_KEY, String(now));
    } catch {
      // The check can still run; it may repeat on the next launch.
    }

    let disposed = false;
    void managerApi.checkManagerUpdate().then((result) => {
      if (!disposed && result.kind === "available") setUpdate(result);
    });
    return () => {
      disposed = true;
    };
  }, []);

  const openGuide = async () => {
    try {
      await managerApi.openUrl(MANAGER_UPDATE_GUIDE_URL);
      setUpdate(null);
    } catch {
      // Keep the prompt open so the user can retry opening the guide.
    }
  };

  return (
    <Sheet
      open={Boolean(update)}
      onDismiss={() => setUpdate(null)}
      labelledBy={titleId}
      describedBy={bodyId}
      initialFocus="dismiss"
    >
      <Ring icon="arrowUp" />
      <h3 id={titleId}>
        {update ? t("about.mgrFound", { version: update.version }) : ""}
      </h3>
      <p id={bodyId}>{t("about.mgrConfirmBody")}</p>
      <div className="row2 sheet-actions">
        <button className="btn ghost" onClick={() => setUpdate(null)}>
          {t("confirm.cancel")}
        </button>
        <button className="btn primary" onClick={() => void openGuide()}>
          {t("about.openDownloadGuide")}
        </button>
      </div>
    </Sheet>
  );
}
