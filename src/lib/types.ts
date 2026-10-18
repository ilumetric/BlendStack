export type Channel = "stable" | "daily" | "experimental" | "custom";
export type RemoteChannel = Exclude<Channel, "custom">;

export interface RemoteBuild {
  id: string;
  channel: Channel;
  version: string;
  releaseCycle: string;
  branch: string;
  patch: string | null;
  hash: string | null;
  url: string;
  fileName: string;
  fileSize: number;
  date: number;
  sha256Url: string | null;
  lts: boolean;
}

export interface InstalledBuild {
  id: string;
  path: string;
  executable: string;
  channel: Channel;
  version: string;
  releaseCycle: string;
  branch: string;
  patch: string | null;
  hash: string | null;
  date: number | null;
  installedAt: number;
  sourceId: string | null;
  archiveSize: number | null;
  lts: boolean;
  external: boolean;
}

export interface BuildList {
  builds: RemoteBuild[];
  fetchedAt: number;
  error: string | null;
}

export interface NewsItem {
  id: string;
  source: "blender" | "developers" | string;
  title: string;
  link: string;
  date: number;
  summary: string;
  image: string | null;
  category: string | null;
  author: string | null;
}

export interface NewsList {
  items: NewsItem[];
  fetchedAt: number;
  error: string | null;
}

export type TaskState =
  | "queued"
  | "downloading"
  | "verifying"
  | "extracting"
  | "done"
  | "failed"
  | "cancelled";

export interface DownloadTask {
  id: string;
  build: RemoteBuild;
  state: TaskState;
  received: number;
  total: number;
  speed: number;
  progress: number;
  error: string | null;
  installedId: string | null;
  replaces: string | null;
  startedAt: number;
}

export interface RecentFile {
  path: string;
  name: string;
  folder: string;
  modified: number;
  size: number;
  series: string;
}

export interface AppNotification {
  id: string;
  kind: "release" | "build" | "update" | "app" | string;
  title: string;
  body: string;
  date: number;
  read: boolean;
  channel: Channel | null;
  buildId: string | null;
}

export interface Settings {
  libraryPath: string | null;
  theme: "system" | "light" | "dark";
  language: string;
  translucency: boolean;
  checkIntervalMinutes: number;
  notifyStable: boolean;
  notifyDaily: boolean;
  notifyDailyAll: boolean;
  notifyExperimental: boolean;
  launchAtLogin: boolean;
  startMinimized: boolean;
  closeToTray: boolean;
  minimizeOnLaunch: boolean;
  showLegacyStable: boolean;
  keepArchives: boolean;
  replaceOnUpdate: boolean;
  showConsole: boolean;
  globalArgs: string;
  defaultBuild: string | null;
  favorites: string[];
  labels: Record<string, string>;
  buildArgs: Record<string, string>;
  lastLaunched: Record<string, number>;
  externalBuilds: string[];
}

export interface SystemInfo {
  os: "windows" | "macos" | "linux" | string;
  arch: string;
  appVersion: string;
  translucent: boolean;
  defaultLibrary: string;
  library: string;
  repository: string;
  /** Settings, cache and logs live next to the executable. */
  selfContained: boolean;
  dataDir: string;
}

export interface Bootstrap {
  settings: Settings;
  system: SystemInfo;
  installed: InstalledBuild[];
  downloads: DownloadTask[];
  notifications: AppNotification[];
}

export interface AppUpdate {
  version: string;
  url: string;
  notes: string;
  /** Can be installed from inside the app (self-contained Windows build). */
  canInstall: boolean;
  size: number;
}
