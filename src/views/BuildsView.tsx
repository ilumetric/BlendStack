import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronRight, CloudOff, FileText, GitPullRequest, Loader2, MoreHorizontal, RefreshCw, Search } from "lucide-react";
import { useStore } from "../store";
import { useT, type Translate } from "../i18n";
import { api } from "../lib/api";
import { compareVersions, normalizeCycle, prNumber, prUrl, releaseNotesUrl, series, tone } from "../lib/builds";
import { formatBytes, formatDate, relativeTime } from "../lib/format";
import { buildName, cycleLabel } from "../lib/naming";
import type { RemoteBuild, RemoteChannel } from "../lib/types";
import { Segmented } from "../components/Segmented";
import { Glyph } from "../components/bits";
import { BuildAction } from "../components/BuildAction";
import { openContextMenu, openMenuAt } from "../components/Menu";
import { remoteMenu } from "./buildMenu";

interface Group {
  key: string;
  title: string;
  tags: { text: string; tone: string }[];
  link?: { label: string; url: string };
  builds: RemoteBuild[];
  /** Stable groups collapse older patch releases. */
  collapsible: boolean;
}

function groupBuilds(channel: RemoteChannel, builds: RemoteBuild[], tr: Translate): Group[] {
  if (channel === "stable") {
    const bySeries = new Map<string, RemoteBuild[]>();
    for (const b of builds) {
      const s = series(b.version);
      bySeries.set(s, [...(bySeries.get(s) ?? []), b]);
    }
    return [...bySeries.entries()]
      .sort((a, b) => compareVersions(b[0], a[0]))
      .map(([s, list]) => ({
        key: s,
        title: tr("builds.series", { series: s }),
        tags: list[0].lts ? [{ text: tr("badge.lts"), tone: "green" }] : [],
        link: { label: tr("menu.releaseNotes"), url: releaseNotesUrl(list[0].version) },
        builds: list.sort((a, b) => compareVersions(b.version, a.version)),
        collapsible: true,
      }));
  }
  if (channel === "daily") {
    const order = ["alpha", "beta", "candidate", "stable"];
    const byCycle = new Map<string, RemoteBuild[]>();
    for (const b of builds) {
      const c = normalizeCycle(b.releaseCycle);
      byCycle.set(c, [...(byCycle.get(c) ?? []), b]);
    }
    return order
      .filter((c) => byCycle.has(c))
      .map((c) => ({
        key: c,
        title: c === "stable" ? tr("builds.maintenance") : cycleLabel(c, tr),
        tags: [],
        builds: byCycle.get(c)!.sort((a, b) => compareVersions(b.version, a.version) || b.date - a.date),
        collapsible: false,
      }));
  }
  const sorted = [...builds].sort((a, b) => b.date - a.date);
  return [{ key: "all", title: "", tags: [], builds: sorted, collapsible: false }];
}

function RemoteRow({
  build,
  isNew,
  focused,
}: {
  build: RemoteBuild;
  isNew: boolean;
  focused: boolean;
}) {
  const tr = useT();
  const lang = useStore((s) => s.lang);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (focused) ref.current?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [focused]);

  const pr = prNumber(build.patch);
  const meta: string[] = [];
  if (build.channel === "stable") {
    meta.push(formatDate(build.date, lang));
  } else {
    meta.push(build.version);
    if (build.channel === "daily") meta.push(build.branch);
    if (build.hash) meta.push(build.hash.slice(0, 7));
    meta.push(relativeTime(build.date, lang));
  }
  meta.push(formatBytes(build.fileSize, lang));

  const title = pr ? tr("builds.pr", { number: pr }) : buildName(build, tr);

  return (
    <div
      ref={ref}
      className={`row${focused ? " focus-flash" : ""}`}
      onContextMenu={(e) => openContextMenu(e, remoteMenu(build))}
    >
      <Glyph build={build} />
      <div className="row-main">
        <div className="row-title">
          {isNew && <span className="new-dot" title={tr("badge.new")} />}
          <span className="name">{title}</span>
          {build.channel === "experimental" && (
            <span className={`tag tone-${tone(build)}`}>{cycleLabel(build.releaseCycle, tr, true)}</span>
          )}
          {build.lts && build.channel !== "stable" && <span className="tag tone-green">{tr("badge.lts")}</span>}
        </div>
        <div className="row-sub">
          {meta.map((m, i) => (
            <span key={i} className={i ? "sep" : undefined}>
              {i ? " " : ""}
              {m}
            </span>
          ))}
        </div>
      </div>
      <div className="row-actions">
        {pr && (
          <button className="icon-btn hover-only" title={tr("menu.openPr")} onClick={() => api.openUrl(prUrl(build.patch)!)}>
            <GitPullRequest size={15} />
          </button>
        )}
        <button
          className="icon-btn hover-only"
          aria-label="More"
          onClick={(e) => openMenuAt(e.currentTarget, remoteMenu(build), true)}
        >
          <MoreHorizontal size={17} />
        </button>
        <BuildAction build={build} />
      </div>
    </div>
  );
}

function GroupCard({ group, prevSeen, focus }: { group: Group; prevSeen: number; focus: string | null }) {
  const tr = useT();
  const [expanded, setExpanded] = useState(false);
  const installed = useStore((s) => s.installed);
  const hasFocus = !!focus && group.builds.some((b) => b.id === focus);
  const hasInstalledOld = group.builds.slice(1).some((b) => installed.some((i) => i.sourceId === b.id));
  const showAll = !group.collapsible || expanded || hasFocus || hasInstalledOld;
  const visible = showAll ? group.builds : group.builds.slice(0, 1);
  const hidden = group.builds.length - visible.length;

  return (
    <section className="card series-group">
      {group.title && (
        <div className="series-head">
          <h3>{group.title}</h3>
          {group.tags.map((t) => (
            <span key={t.text} className={`tag tone-${t.tone}`}>
              {t.text}
            </span>
          ))}
          <span className="spacer" />
          {group.collapsible && group.builds.length > 1 && !hasFocus && !hasInstalledOld && (
            <button className="btn plain" onClick={() => setExpanded((v) => !v)}>
              {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} className="flip-rtl" />}
              {expanded ? tr("builds.hideOlder") : tr("builds.showOlder", { count: hidden })}
            </button>
          )}
          {group.link && (
            <button className="btn plain" onClick={() => api.openUrl(group.link!.url)}>
              <FileText size={13} />
              {group.link.label}
            </button>
          )}
        </div>
      )}
      {visible.map((b, i) => (
        <div key={b.id}>
          {i > 0 && <div className="list-divider" />}
          <RemoteRow build={b} isNew={b.date > prevSeen} focused={b.id === focus} />
        </div>
      ))}
    </section>
  );
}

export function BuildsView() {
  const tr = useT();
  const lang = useStore((s) => s.lang);
  const channel = useStore((s) => s.buildsChannel);
  const setChannel = useStore((s) => s.setBuildsChannel);
  const remote = useStore((s) => s.remote);
  const loadBuilds = useStore((s) => s.loadBuilds);
  const prevSeen = useStore((s) => s.prevSeen);
  const seen = useStore((s) => s.seen);
  const focus = useStore((s) => s.focusBuild);
  const [query, setQuery] = useState("");
  const { list, loading } = remote[channel];

  useEffect(() => {
    if (!remote[channel].list && !remote[channel].loading) void loadBuilds(channel);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [channel]);

  useEffect(() => {
    if (!focus) return;
    const id = window.setTimeout(() => useStore.setState({ focusBuild: null }), 2500);
    return () => window.clearTimeout(id);
  }, [focus]);

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const builds = (list?.builds ?? []).filter((b) => {
      if (!q) return true;
      const hay = `${b.version} ${b.branch} ${b.hash ?? ""} ${b.patch ?? ""} ${prNumber(b.patch) ?? ""} ${b.releaseCycle}`;
      return hay.toLowerCase().includes(q);
    });
    return groupBuilds(channel, builds, tr);
  }, [list, query, channel, tr]);

  const newCount = (c: RemoteChannel) =>
    c === channel ? 0 : (remote[c].list?.builds ?? []).filter((b) => b.date > seen[c]).length;

  return (
    <div className="view builds">
      <div className="toolbar">
        <Segmented<RemoteChannel>
          value={channel}
          onChange={(c) => {
            setChannel(c);
            setQuery("");
          }}
          segments={[
            { value: "stable", label: tr("channel.stable"), badge: newCount("stable") },
            { value: "daily", label: tr("channel.daily"), badge: newCount("daily") },
            { value: "experimental", label: tr("channel.experimental") },
          ]}
        />
        <div className="toolbar-status">
          {!loading && list?.fetchedAt ? tr("builds.updated", { time: relativeTime(list.fetchedAt, lang) }) : null}
        </div>
        <div className="search">
          <Search size={13} />
          <input
            className="input"
            placeholder={tr("builds.search")}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <button className="icon-btn" title={`${tr("common.refresh")} (Ctrl+R)`} onClick={() => loadBuilds(channel, true)}>
          <RefreshCw size={16} className={loading ? "spin" : undefined} />
        </button>
      </div>

      <div className="channel-intro">{tr(`builds.about.${channel}`)}</div>

      {list?.error && (
        <div className="banner">
          <CloudOff size={15} />
          {tr("builds.offline")}
        </div>
      )}

      <div className="builds-scroll">
        {!list && loading && (
          <div className="placeholder">
            <Loader2 size={26} className="spin" />
            {tr("builds.loading")}
          </div>
        )}
        {list && groups.every((g) => g.builds.length === 0) && <div className="placeholder">{tr("builds.empty")}</div>}
        {groups
          .filter((g) => g.builds.length)
          .map((g) => (
            <GroupCard key={`${channel}-${g.key}`} group={g} prevSeen={prevSeen[channel]} focus={focus} />
          ))}
      </div>
    </div>
  );
}

