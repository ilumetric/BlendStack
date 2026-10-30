import { useEffect, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ArrowUpCircle, ExternalLink, FolderOpen, X } from "lucide-react";
import { useStore } from "../store";
import { LANGUAGES, useT } from "../i18n";
import { api, errorText } from "../lib/api";
import type { Settings } from "../lib/types";
import { Backdrop, confirmDialog } from "../components/Dialogs";
import { Switch } from "../components/bits";
import { Select } from "../components/Select";
import { revealLabel } from "./buildMenu";
import logo from "../assets/logo.svg";

const INTERVALS = [0, 15, 60, 180, 720, 1440] as const;
/** Opening Settings re-asks GitHub at most this often. */
const UPDATE_RECHECK_MS = 10 * 60 * 1000;

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
  const update = useStore((s) => s.appUpdate);
  const updateChecked = useStore((s) => s.appUpdateChecked);
  const checking = useStore((s) => s.appUpdateChecking);
  const installing = useStore((s) => s.appUpdateProgress);
  const blendHandler = useStore((s) => s.blendHandler);
  const [args, setArgs] = useState(settings?.globalArgs ?? "");

  useEffect(() => {
    const s = useStore.getState();
    if (Date.now() - s.appUpdateChecked > UPDATE_RECHECK_MS) void s.checkAppUpdate();
  }, []);

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

  const installUpdate = () => void useStore.getState().installAppUpdate();
  const checkUpdate = () => void useStore.getState().checkAppUpdate(true);

  const updateButton = update ? (
    update.canInstall ? (
      <button className="btn primary" onClick={installUpdate} disabled={installing !== null}>
        {installing === null
          ? tr("settings.updateNow")
          : installing >= 1
            ? tr("settings.restarting")
            : `${tr("settings.installing")} ${Math.round(installing * 100)}%`}
      </button>
    ) : (
      <button className="btn primary" onClick={() => api.openUrl(update.url)}>
        {tr("settings.download")}
        <ExternalLink size={12} />
      </button>
    )
  ) : null;

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
          {update && (
            <div className="update-banner">
              <ArrowUpCircle size={22} className="update-banner-icon" />
              <div className="update-banner-text">
                <div className="update-banner-title">{tr("settings.updateBanner", { version: update.version })}</div>
                <div className="muted">
                  {tr(update.canInstall ? "settings.updateBanner.hint" : "settings.updateBanner.hintManual", {
                    current: system.appVersion,
                  })}
                </div>
                {installing !== null && (
                  <div className="update-progress">
                    <div style={{ width: `${Math.round(installing * 100)}%` }} />
                  </div>
                )}
              </div>
              {updateButton}
            </div>
          )}
          <Group title={tr("settings.appearance")}>
            <Row label={tr("settings.theme")}>
              <Select<Settings["theme"]>
                value={settings.theme}
                onChange={(theme) => patch({ theme })}
                options={[
                  { value: "system", label: tr("settings.theme.system") },
                  { value: "light", label: tr("settings.theme.light") },
                  { value: "dark", label: tr("settings.theme.dark") },
                ]}
              />
            </Row>
            <Row label={tr("settings.language")}>
              <Select
                value={settings.language}
                onChange={(language) => patch({ language })}
                options={[
                  { value: "", label: tr("settings.language.auto") },
                  ...LANGUAGES.map((l) => ({ value: l.code, label: l.name })),
                ]}
              />
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
              <Select<number>
                value={settings.checkIntervalMinutes}
                onChange={(checkIntervalMinutes) => patch({ checkIntervalMinutes })}
                options={INTERVALS.map((m) => ({ value: m, label: tr(`settings.interval.${m}`) }))}
              />
            </Row>
            <Row label={tr("settings.notifyStable")}>{toggle("notifyStable")}</Row>
            <Row label={tr("settings.notifyDaily")}>{toggle("notifyDaily")}</Row>
            {settings.notifyDaily && <Row label={tr("settings.notifyDailyAll")}>{toggle("notifyDailyAll")}</Row>}
            <Row label={tr("settings.notifyExperimental")}>{toggle("notifyExperimental")}</Row>
            <Row label={tr("settings.notifyChat")} hint={tr("settings.notifyChat.hint")}>
              {toggle("notifyChat")}
            </Row>
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
            <Row
              label={tr("settings.dataFolder")}
              hint={
                <>
                  <span className="path" title={system.dataDir}>
                    {system.dataDir}
                  </span>
                  {system.selfContained && tr("settings.dataFolder.selfContained")}
                </>
              }
            >
              <button className="icon-btn" title={revealLabel(tr)} onClick={() => api.reveal(system.dataDir).catch(() => {})}>
                <FolderOpen size={16} />
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
            {system.os === "windows" && (
              <Row
                label={tr("settings.blendHandler")}
                hint={
                  blendHandler.registered && !blendHandler.active
                    ? tr("settings.blendHandler.confirm")
                    : tr("settings.blendHandler.hint")
                }
              >
                {blendHandler.registered && !blendHandler.active && (
                  <button className="btn" onClick={() => void api.openDefaultApps().catch(() => {})}>
                    {tr("settings.defaultApps")}
                  </button>
                )}
                <Switch
                  on={blendHandler.registered}
                  onChange={(v) => void useStore.getState().setBlendHandler(v)}
                />
              </Row>
            )}
            {system.os === "windows" && blendHandler.registered && (
              <Row label={tr("settings.askForFiles")} hint={tr("settings.askForFiles.hint")}>
                {toggle("askForFiles")}
              </Row>
            )}
          </Group>

          <Group title={tr("settings.about")}>
            <div className="about">
              <img src={logo} alt="" />
              <div style={{ flex: 1 }}>
                <div style={{ fontWeight: 700, fontSize: 15 }}>Blend Stack</div>
                <div className="muted">{tr("settings.version", { version: system.appVersion })}</div>
                {!update && updateChecked > 0 && !checking && (
                  <div className="muted">{tr("settings.upToDate")}</div>
                )}
                {update && (
                  <div style={{ color: "var(--accent)", fontWeight: 600 }}>
                    {tr("settings.updateAvailable", { version: update.version })}
                  </div>
                )}
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 6, alignItems: "flex-end" }}>
                {updateButton ?? (
                  <button className="btn" onClick={checkUpdate} disabled={checking}>
                    {checking ? tr("settings.checking") : tr("settings.checkUpdate")}
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
