import { useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ExternalLink, FolderOpen, X } from "lucide-react";
import { useStore } from "../store";
import { LANGUAGES, useT } from "../i18n";
import { api, errorText } from "../lib/api";
import type { AppUpdate, Settings } from "../lib/types";
import { Backdrop, confirmDialog } from "../components/Dialogs";
import { Switch } from "../components/bits";
import { revealLabel } from "./buildMenu";
import logo from "../assets/logo.svg";

const INTERVALS = [0, 15, 60, 180, 720, 1440] as const;

function Row({ label, hint, children }: { label: ReactNode; hint?: ReactNode; children?: ReactNode }) {
  return (
    <div className="form-row">
      <div className="label">
        {label}
        {hint && <span className="hint">{hint}</span>}
      </div>
      {children}
    </div>
  );
}

function Group({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="form-group">
      <h4>{title}</h4>
      <div className="form-card">{children}</div>
    </div>
  );
}

export function SettingsSheet() {
  const tr = useT();
  const settings = useStore((s) => s.settings);
  const system = useStore((s) => s.system);
  const patch = useStore((s) => s.patchSettings);
  const close = () => useStore.getState().setSettingsOpen(false);
  const [update, setUpdate] = useState<AppUpdate | null | "checking" | "none">(null);
  const [args, setArgs] = useState(settings?.globalArgs ?? "");

  if (!settings || !system) return null;

  const toggle = (key: keyof Settings) => (
    <Switch on={!!settings[key]} onChange={(v) => patch({ [key]: v } as Partial<Settings>)} />
  );

  const switchLibrary = async (target: string | null) => {
    const s = useStore.getState();
    const managed = s.installed.filter((b) => !b.external).length;
    const move =
      managed > 0 &&
      (await confirmDialog({
        title: tr("settings.move.title"),
        body: tr("settings.move.body", { count: managed }),
        confirmLabel: tr("settings.move.confirm"),
        cancelLabel: tr("settings.move.skip"),
      }));
    try {
      if (move) {
        s.toast(tr("toast.moving"), "info");
        const moved = await api.moveLibrary(target);
        s.toast(tr("toast.moved", { count: moved }), "success");
      } else {
        await patch({ libraryPath: target });
      }
      useStore.setState({ system: await api.systemInfo() });
    } catch (e) {
      s.toast(errorText(e), "error");
    }
  };

  const chooseLibrary = async () => {
    const dir = await open({ directory: true, defaultPath: system.library });
    if (typeof dir === "string" && dir !== system.library) await switchLibrary(dir);
  };

  const checkUpdate = async () => {
    setUpdate("checking");
    try {
      setUpdate((await api.checkAppUpdate()) ?? "none");
    } catch (e) {
      setUpdate(null);
      useStore.getState().toast(errorText(e), "error");
    }
  };

  return (
    <Backdrop onClose={close}>
      <div className="sheet" role="dialog" aria-label={tr("settings.title")}>
        <div className="sheet-head">
          <h2>{tr("settings.title")}</h2>
          <button className="icon-btn" onClick={close} aria-label={tr("common.close")}>
            <X size={17} />
          </button>
        </div>
        <div className="sheet-body">
          <Group title={tr("settings.appearance")}>
            <Row label={tr("settings.theme")}>
              <select
                className="select"
                value={settings.theme}
                onChange={(e) => patch({ theme: e.target.value as Settings["theme"] })}
              >
                <option value="system">{tr("settings.theme.system")}</option>
                <option value="light">{tr("settings.theme.light")}</option>
                <option value="dark">{tr("settings.theme.dark")}</option>
              </select>
            </Row>
            <Row label={tr("settings.language")}>
              <select className="select" value={settings.language} onChange={(e) => patch({ language: e.target.value })}>
                <option value="">{tr("settings.language.auto")}</option>
                {LANGUAGES.map((l) => (
                  <option key={l.code} value={l.code}>
                    {l.name}
                  </option>
                ))}
              </select>
            </Row>
            {system.os !== "linux" && (
              <Row label={tr("settings.translucency")} hint={tr("settings.translucency.hint")}>
                {toggle("translucency")}
              </Row>
            )}
          </Group>

          <Group title={tr("settings.general")}>
            <Row label={tr("settings.launchAtLogin")}>{toggle("launchAtLogin")}</Row>
            {settings.launchAtLogin && <Row label={tr("settings.startMinimized")}>{toggle("startMinimized")}</Row>}
            <Row label={tr("settings.closeToTray")}>{toggle("closeToTray")}</Row>
            <Row label={tr("settings.minimizeOnLaunch")}>{toggle("minimizeOnLaunch")}</Row>
          </Group>

          <Group title={tr("settings.updates")}>
            <Row label={tr("settings.checkInterval")}>
              <select
                className="select"
                value={settings.checkIntervalMinutes}
                onChange={(e) => patch({ checkIntervalMinutes: Number(e.target.value) })}
              >
                {INTERVALS.map((m) => (
                  <option key={m} value={m}>
                    {tr(`settings.interval.${m}`)}
                  </option>
                ))}
              </select>
            </Row>
            <Row label={tr("settings.notifyStable")}>{toggle("notifyStable")}</Row>
            <Row label={tr("settings.notifyDaily")}>{toggle("notifyDaily")}</Row>
            {settings.notifyDaily && <Row label={tr("settings.notifyDailyAll")}>{toggle("notifyDailyAll")}</Row>}
            <Row label={tr("settings.notifyExperimental")}>{toggle("notifyExperimental")}</Row>
          </Group>

          <Group title={tr("settings.library")}>
            <Row
              label={tr("settings.libraryPath")}
              hint={
                <>
                  <span className="path" title={system.library}>
                    {system.library}
                  </span>
                  {tr("settings.libraryPath.hint")}
                </>
              }
            >
              <button className="icon-btn" title={revealLabel(tr)} onClick={() => api.reveal(system.library).catch(() => {})}>
                <FolderOpen size={16} />
              </button>
              {settings.libraryPath && (
                <button className="btn" onClick={() => switchLibrary(null)}>
                  {tr("common.reset")}
                </button>
              )}
              <button className="btn" onClick={chooseLibrary}>
                {tr("common.change")}
              </button>
            </Row>
            <Row label={tr("settings.replaceOnUpdate")}>{toggle("replaceOnUpdate")}</Row>
            <Row label={tr("settings.keepArchives")}>{toggle("keepArchives")}</Row>
            <Row label={tr("settings.showLegacy")}>
              <Switch
                on={settings.showLegacyStable}
                onChange={async (v) => {
                  await patch({ showLegacyStable: v });
                  void useStore.getState().loadBuilds("stable", true);
                }}
              />
            </Row>
          </Group>

          <Group title={tr("settings.launching")}>
            <Row label={tr("settings.globalArgs")}>
              <input
                className="input mono"
                style={{ width: 220 }}
                value={args}
                placeholder={tr("settings.globalArgs.placeholder")}
                spellCheck={false}
                onChange={(e) => setArgs(e.target.value)}
                onBlur={() => args !== settings.globalArgs && patch({ globalArgs: args })}
              />
            </Row>
            {system.os === "windows" && <Row label={tr("settings.showConsole")}>{toggle("showConsole")}</Row>}
          </Group>

          <Group title={tr("settings.about")}>
            <div className="about">
              <img src={logo} alt="" />
              <div style={{ flex: 1 }}>
                <div style={{ fontWeight: 700, fontSize: 15 }}>Blend Stack</div>
                <div className="muted">{tr("settings.version", { version: system.appVersion })}</div>
                {update === "none" && <div className="muted">{tr("settings.upToDate")}</div>}
                {update && typeof update === "object" && (
                  <div style={{ color: "var(--accent)", fontWeight: 600 }}>
                    {tr("settings.updateAvailable", { version: update.version })}
                  </div>
                )}
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 6, alignItems: "flex-end" }}>
                {update && typeof update === "object" ? (
                  <button className="btn primary" onClick={() => api.openUrl(update.url)}>
                    {tr("settings.download")}
                  </button>
                ) : (
                  <button className="btn" onClick={checkUpdate} disabled={update === "checking"}>
                    {tr("settings.checkUpdate")}
                  </button>
                )}
                <button className="btn plain" onClick={() => api.openUrl(system.repository)}>
                  {tr("settings.source")}
                  <ExternalLink size={12} />
                </button>
              </div>
            </div>
          </Group>
          <p className="disclaimer">{tr("settings.disclaimer")}</p>
        </div>
      </div>
    </Backdrop>
  );
}
