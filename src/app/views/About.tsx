import { useCallback, useId, useState } from "react";

import {
  MANAGER_UPDATE_GUIDE_URL,
  managerApi,
  type ManagerUpdateAvailable,
} from "../../services/managerApi";
import { userErrorMessage } from "../errorCopy";
import { Icon } from "../icons";
import { useI18n } from "../i18n";
import { NavBar, Ring } from "../components";
import { formatDiagnostics } from "../diagnostics";
import { Sheet } from "../Sheet";

const APP_VERSION = import.meta.env.VITE_APP_VERSION ?? "0.0.0";

export function About({ onBack }: { onBack: () => void }) {
  const { t } = useI18n();
  const [mgrBusy, setMgrBusy] = useState(false);
  const [mgrMsg, setMgrMsg] = useState<string | null>(null);
  const [pendingUpdate, setPendingUpdate] = useState<ManagerUpdateAvailable | null>(null);
  const updateTitleId = useId();
  const updateBodyId = useId();

  const closeUpdateConfirm = useCallback(() => {
    if (mgrBusy) return;
    setPendingUpdate(null);
  }, [mgrBusy]);

  const checkManager = useCallback(async () => {
    setMgrBusy(true);
    setMgrMsg(null);
    if (pendingUpdate) {
      setPendingUpdate(null);
    }
    try {
      const result = await managerApi.checkManagerUpdate();
      if (result.kind === "available") {
        setPendingUpdate(result);
        setMgrMsg(t("about.mgrFound", { version: result.version }));
      } else if (result.kind === "none") {
        setMgrMsg(t("about.mgrUpToDate"));
      } else if (result.kind === "development") {
        setMgrMsg(t("about.mgrDev"));
      } else {
        setMgrMsg(t("about.mgrUnavailable"));
      }
    } catch (cause) {
      setMgrMsg(userErrorMessage(cause, t));
    } finally {
      setMgrBusy(false);
    }
  }, [pendingUpdate, t]);

  const openManagerUpdateGuide = useCallback(async () => {
    if (!pendingUpdate) return;
    setMgrBusy(true);
    try {
      await managerApi.openUrl(MANAGER_UPDATE_GUIDE_URL);
      setPendingUpdate(null);
    } catch (cause) {
      setMgrMsg(userErrorMessage(cause, t));
    } finally {
      setMgrBusy(false);
    }
  }, [pendingUpdate, t]);

  const openLogsDir = useCallback(async () => {
    setMgrMsg(null);
    try {
      await managerApi.openLogsDir();
    } catch (cause) {
      setMgrMsg(userErrorMessage(cause, t));
    }
  }, [t]);

  const copyDiagnostics = useCallback(async () => {
    setMgrMsg(null);
    try {
      const diagnostics = await managerApi.getDiagnostics();
      await navigator.clipboard.writeText(formatDiagnostics(diagnostics));
      setMgrMsg(t("about.diagnosticsCopied"));
    } catch {
      setMgrMsg(t("about.diagnosticsFailed"));
    }
  }, [t]);

  return (
    <div className="pop">
      <NavBar title={t("settings.more.about")} onBack={onBack} disableBack={mgrBusy} />
      <div className="scroll view" inert={pendingUpdate ? true : undefined}>
        <section className="hero" style={{ paddingTop: 8 }}>
          <div className="headline" style={{ fontSize: 18 }}>
            {t("app.name")}
          </div>
          <div className="sub">{t("about.version", { v: APP_VERSION })}</div>
          <div className="desc">{t("about.tagline")}</div>
        </section>

        <div className="list">
          <button className="row" onClick={checkManager} disabled={mgrBusy}>
            <Icon name="refresh" className="ricon" />
            <span className="rtext">
              <span className="rtitle">{t("about.checkManager")}</span>
              {mgrMsg ? <span className="rsub">{mgrMsg}</span> : null}
            </span>
            <span className="rval">{mgrBusy ? t("about.mgrChecking") : ""}</span>
          </button>
          <button className="row" onClick={openLogsDir}>
            <Icon name="folder" className="ricon" />
            <span className="rtext">
              <span className="rtitle">{t("about.openLogsDir")}</span>
            </span>
            <Icon name="chevron" className="chev" />
          </button>
          <button className="row" onClick={copyDiagnostics}>
            <Icon name="copy" className="ricon" />
            <span className="rtext">
              <span className="rtitle">{t("about.copyDiagnostics")}</span>
            </span>
            <Icon name="chevron" className="chev" />
          </button>
        </div>
      </div>
      <Sheet
        open={Boolean(pendingUpdate)}
        onDismiss={closeUpdateConfirm}
        dismissable={!mgrBusy}
        labelledBy={updateTitleId}
        describedBy={updateBodyId}
        initialFocus="dismiss"
      >
        <Ring icon="arrowUp" />
        <h3 id={updateTitleId}>
          {pendingUpdate ? t("confirm.title", { version: pendingUpdate.version }) : ""}
        </h3>
        <p id={updateBodyId}>{t("about.mgrConfirmBody")}</p>
        <div className="row2 sheet-actions">
          <button className="btn ghost" onClick={closeUpdateConfirm} disabled={mgrBusy}>
            {t("confirm.cancel")}
          </button>
          <button className="btn primary" onClick={openManagerUpdateGuide} disabled={mgrBusy}>
            {t("about.openDownloadGuide")}
          </button>
        </div>
      </Sheet>
    </div>
  );
}
