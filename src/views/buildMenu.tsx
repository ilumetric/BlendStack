import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowUpCircle,
  Copy,
  FileText,
  FolderOpen,
  GitPullRequest,
  Pencil,
  Play,
  Star,
  StarOff,
  Terminal,
  Trash2,
  CheckCircle2,
  FileCheck2,
} from "lucide-react";
import { useStore } from "../store";
import { t, type Translate } from "../i18n";
import { api } from "../lib/api";
import { prUrl, releaseNotesUrl } from "../lib/builds";
import { buildName } from "../lib/naming";
import type { InstalledBuild, RemoteBuild } from "../lib/types";
import type { MenuEntry } from "../components/Menu";
import { confirmDialog, promptDialog } from "../components/Dialogs";

export function revealLabel(tr: Translate): string {
  const os = useStore.getState().system?.os;
  return tr(os === "macos" ? "menu.reveal.macos" : os === "linux" ? "menu.reveal.linux" : "menu.reveal.windows");
}

export async function pickBlendFile(): Promise<string | null> {
  const file = await open({ multiple: false, filters: [{ name: "Blender", extensions: ["blend"] }] });
  return typeof file === "string" ? file : null;
}

export async function copyText(text: string) {
  await navigator.clipboard.writeText(text);
  useStore.getState().toast(t("toast.copied"), "success");
}

export async function deleteWithConfirm(build: InstalledBuild) {
  const s = useStore.getState();
  const name = buildName(build, t, s.settings?.labels);
  const ok = await confirmDialog(
    build.external
      ? {
          title: t("dialog.remove.title", { name }),
          body: t("dialog.remove.body"),
          confirmLabel: t("common.remove"),
          danger: true,
        }
      : {
          title: t("dialog.delete.title", { name }),
          body: t("dialog.delete.body"),
          confirmLabel: t("common.delete"),
          danger: true,
        },
  );
  if (ok) await s.removeBuild(build);
}

export function installedMenu(build: InstalledBuild, update: RemoteBuild | null): MenuEntry[] {
  const s = useStore.getState();
  const settings = s.settings;
  const favorite = settings?.favorites.includes(build.id) ?? false;
  const isDefault = settings?.defaultBuild === build.id;
  const entries: MenuEntry[] = [
    { label: t("menu.launch"), icon: <Play size={14} />, onSelect: () => s.launch(build) },
    {
      label: t("menu.launchWithFile"),
      icon: <FileText size={14} />,
      onSelect: async () => {
        const file = await pickBlendFile();
        if (file) await s.launch(build, file);
      },
    },
  ];
  if (update) {
    entries.push({
      label: t("library.updateTo", { version: update.version }),
      icon: <ArrowUpCircle size={14} />,
      onSelect: () => s.download(update, build.id),
    });
  }
  entries.push(
    { separator: true },
    {
      label: t("menu.setDefault"),
      icon: <CheckCircle2 size={14} />,
      checked: isDefault,
      onSelect: () => s.patchSettings({ defaultBuild: isDefault ? null : build.id }),
    },
    {
      label: favorite ? t("menu.unfavorite") : t("menu.favorite"),
      icon: favorite ? <StarOff size={14} /> : <Star size={14} />,
      onSelect: () =>
        s.patchSettings({
          favorites: favorite
            ? (settings?.favorites ?? []).filter((f) => f !== build.id)
            : [...(settings?.favorites ?? []), build.id],
        }),
    },
    {
      label: t("menu.rename"),
      icon: <Pencil size={14} />,
      onSelect: async () => {
        const value = await promptDialog({
          title: t("dialog.rename.title"),
          placeholder: buildName(build, t),
          value: settings?.labels[build.id] ?? "",
        });
        if (value === null) return;
        const labels = { ...(useStore.getState().settings?.labels ?? {}) };
        if (value.trim()) labels[build.id] = value.trim();
        else delete labels[build.id];
        await s.patchSettings({ labels });
      },
    },
    {
      label: t("menu.args"),
      icon: <Terminal size={14} />,
      onSelect: async () => {
        const value = await promptDialog({
          title: t("dialog.args.title"),
          hint: t("dialog.args.hint"),
          placeholder: "--factory-startup",
          value: settings?.buildArgs[build.id] ?? "",
          mono: true,
        });
        if (value === null) return;
        const buildArgs = { ...(useStore.getState().settings?.buildArgs ?? {}) };
        if (value.trim()) buildArgs[build.id] = value.trim();
        else delete buildArgs[build.id];
        await s.patchSettings({ buildArgs });
      },
    },
    { separator: true },
    { label: revealLabel(t), icon: <FolderOpen size={14} />, onSelect: () => api.reveal(build.path).catch(() => {}) },
    { label: t("menu.copyPath"), icon: <Copy size={14} />, onSelect: () => copyText(build.executable) },
  );
  // Blender registers itself for .blend files on Windows and Linux; this also
  // gives Windows the icon for Blender's taskbar button.
  if (s.system?.os === "windows" || s.system?.os === "linux") {
    const current = s.blendAssociation?.toLowerCase() === build.path.toLowerCase();
    entries.push({
      label: t("menu.associate"),
      icon: <FileCheck2 size={14} />,
      checked: current,
      onSelect: () => s.associateBlendFiles(build),
    });
  }
  if (build.channel === "stable" || build.channel === "custom") {
    entries.push({
      label: t("menu.releaseNotes"),
      icon: <FileText size={14} />,
      onSelect: () => api.openUrl(releaseNotesUrl(build.version)),
    });
  }
  const pr = prUrl(build.patch);
  if (pr) entries.push({ label: t("menu.openPr"), icon: <GitPullRequest size={14} />, onSelect: () => api.openUrl(pr) });
  entries.push(
    { separator: true },
    {
      label: build.external ? t("menu.removeImported") : t("menu.delete"),
      icon: <Trash2 size={14} />,
      danger: true,
      onSelect: () => deleteWithConfirm(build),
    },
  );
  return entries;
}

export function remoteMenu(build: RemoteBuild): MenuEntry[] {
  const entries: MenuEntry[] = [];
  if (build.channel === "stable") {
    entries.push({
      label: t("menu.releaseNotes"),
      icon: <FileText size={14} />,
      onSelect: () => api.openUrl(releaseNotesUrl(build.version)),
    });
  }
  const pr = prUrl(build.patch);
  if (pr) entries.push({ label: t("menu.openPr"), icon: <GitPullRequest size={14} />, onSelect: () => api.openUrl(pr) });
  entries.push({ label: t("menu.copyLink"), icon: <Copy size={14} />, onSelect: () => copyText(build.url) });
  return entries;
}
