import { invoke } from "@tauri-apps/api/core";
import type {
  AppNotification,
  AppUpdate,
  Bootstrap,
  BuildList,
  DownloadTask,
  FileSection,
  InstalledBuild,
  NewsList,
  RecentFile,
  RemoteBuild,
  RemoteChannel,
  Settings,
  SystemInfo,
} from "./types";

/** Typed wrappers around the Rust commands in `src-tauri/src/commands.rs`. */
export const api = {
  bootstrap: () => invoke<Bootstrap>("get_bootstrap"),
  appReady: () => invoke<void>("app_ready"),
  systemInfo: () => invoke<SystemInfo>("get_system_info"),
  updateSettings: (patch: Partial<Settings>) => invoke<Settings>("update_settings", { patch }),

  installed: () => invoke<InstalledBuild[]>("list_installed"),
  cachedBuilds: (channel: RemoteChannel) => invoke<BuildList>("get_cached_builds", { channel }),
  builds: (channel: RemoteChannel, force: boolean) => invoke<BuildList>("get_builds", { channel, force }),
  cachedNews: () => invoke<NewsList>("get_cached_news"),
  news: (force: boolean) => invoke<NewsList>("get_news", { force }),

  startDownload: (build: RemoteBuild, replaces: string | null = null) =>
    invoke<DownloadTask>("start_download", { build, replaces }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  clearDownloads: () => invoke<DownloadTask[]>("clear_downloads"),
  downloads: () => invoke<DownloadTask[]>("list_downloads"),

  launch: (id: string, file: string | null = null) => invoke<void>("launch_build", { id, file }),
  deleteBuild: (id: string) => invoke<void>("delete_build", { id }),
  importBuild: (path: string) => invoke<InstalledBuild>("import_build", { path }),
  moveLibrary: (target: string | null) => invoke<number>("move_library", { target }),
  reveal: (path: string) => invoke<void>("reveal_path", { path }),
  openUrl: (url: string) => invoke<void>("open_url", { url }),

  recentFiles: (section: FileSection) => invoke<RecentFile[]>("get_recent_files", { section }),
  thumbnail: (path: string) => invoke<string | null>("get_thumbnail", { path }),

  notifications: () => invoke<AppNotification[]>("get_notifications"),
  markNotificationsRead: () => invoke<void>("mark_notifications_read"),
  clearNotifications: () => invoke<void>("clear_notifications"),
  checkNow: () => invoke<AppNotification[]>("check_now"),
  takePendingFiles: () => invoke<string[]>("take_pending_files"),
  checkAppUpdate: () => invoke<AppUpdate | null>("check_app_update"),
  installAppUpdate: () => invoke<void>("install_app_update"),
  registerBlendFiles: (id: string) => invoke<void>("register_blend_files", { id }),
  blendAssociation: () => invoke<string | null>("blend_association"),
};

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
