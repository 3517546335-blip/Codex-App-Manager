import { useState } from "react";

import { errorMessage, managerApi, type ProviderConfigReport } from "../../services/managerApi";
import { Icon } from "../icons";
import { useI18n } from "../i18n";
import { NavBar, Ring } from "../components";
import { isWindows } from "../platform";

export function CodexConfig({ onBack }: { onBack: () => void }) {
  const { t } = useI18n();
  const win = isWindows();
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<ProviderConfigReport | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const applyPreset = async () => {
    setBusy(true);
    setErr(null);
    setReport(null);
    try {
      setReport(await managerApi.applyProviderConfig());
    } catch (cause) {
      setReport(null);
      setErr(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="pop">
      <NavBar title={t("nav.config")} onBack={onBack} />
      <div className="scroll view">
        <section className="hero codex-config-hero">
          <Ring icon="sliders" className="glow" />
          <div className="headline" style={{ fontSize: 18 }}>
            一键配置
          </div>
          <div className="desc">
            {win
              ? "写入 DonaAPI 配置，并为 Windows ChatGPT 启用内置中文资源。"
              : "备份现有配置后，写入 DonaAPI 模型配置。"}
          </div>
        </section>

        {err ? (
          <div className="banner err">
            <Icon name="alert" />
            <span>{err}</span>
          </div>
        ) : null}

        {report ? (
          <div className="banner ok">
            <Icon name="check" />
            <span>{report.message}</span>
          </div>
        ) : null}

        <div className="list config-preset">
          <div className="row">
            <Icon name="sliders" className="ricon" />
            <span className="rtext">
              <span className="rtitle">{win ? "配置与 Windows 中文化" : "写入 Codex 配置"}</span>
              <span className="rsub">
                {win
                  ? "先写入配置，再关闭当前 ChatGPT 进程并修改安装包内的中文初始化项。MSIX 可能弹出一次管理员授权。"
                  : "现有配置将备份为 config.toml.bak，然后按下方内容重写。"}
              </span>
            </span>
          </div>
          <div className="config-preview">
            <code>model_provider = "donaapi"</code>
            <code>model = "gpt-5.6-sol"</code>
            <code>review_model = "gpt-5.6-sol"</code>
            <code>model_reasoning_effort = "xhigh"</code>
            <code>[model_providers.donaapi]</code>
            <code>name = "DonaAPI"</code>
            <code>base_url = "https://donaapi.com/v1"</code>
          </div>
        </div>

        <div className="actions">
          <button className="btn primary big" onClick={applyPreset} disabled={busy}>
            <Icon name={busy ? "loader" : "sliders"} className={busy ? "spinicon" : undefined} />
            {busy ? "正在处理…" : "一键配置"}
          </button>
        </div>

        {report ? (
          <div className="list meta config-result">
            <div className="row">
              <span className="rtext">
                <span className="rtitle">配置文件</span>
              </span>
              <span className="rval path" title={report.configPath}>
                {report.configPath}
              </span>
            </div>
            <div className="row">
              <span className="rtext">
                <span className="rtitle">更新项</span>
              </span>
              <span className="rval">{report.changedKeys.length || 0}</span>
            </div>
            <div className="row">
              <span className="rtext">
                <span className="rtitle">新增项</span>
              </span>
              <span className="rval">{report.addedKeys.length || 0}</span>
            </div>
            {report.localization ? (
              <div className="row">
                <span className="rtext">
                  <span className="rtitle">Windows 中文化</span>
                  <span className="rsub">{report.localization.message}</span>
                </span>
                <span className="rval">
                  {report.localization.status === "patched"
                    ? "已完成"
                    : report.localization.status === "already-patched"
                      ? "已启用"
                      : report.localization.status === "not-installed"
                        ? "已跳过"
                        : "未完成"}
                </span>
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}
