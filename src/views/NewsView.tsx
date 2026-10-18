import { useMemo, useState } from "react";
import { CloudOff, Loader2, Newspaper, RefreshCw } from "lucide-react";
import { useStore } from "../store";
import { useT } from "../i18n";
import { api } from "../lib/api";
import { formatDate, relativeTime } from "../lib/format";
import type { NewsItem } from "../lib/types";
import { Segmented } from "../components/Segmented";
import logo from "../assets/logo.svg";

type Source = "all" | "blender" | "developers";

function Cover({ src }: { src: string | null }) {
  const [loaded, setLoaded] = useState(false);
  const [failed, setFailed] = useState(false);
  return (
    <div className="news-image">
      {(!src || failed || !loaded) && <img className="logo" src={logo} alt="" draggable={false} />}
      {src && !failed && (
        <img
          className={`cover${loaded ? " loaded" : ""}`}
          src={src}
          alt=""
          loading="lazy"
          referrerPolicy="no-referrer"
          draggable={false}
          onLoad={() => setLoaded(true)}
          onError={() => setFailed(true)}
        />
      )}
    </div>
  );
}

function NewsCard({ item, featured, isNew }: { item: NewsItem; featured: boolean; isNew: boolean }) {
  const tr = useT();
  const lang = useStore((s) => s.lang);
  const source = item.source === "developers" ? tr("news.developers") : tr("news.blender");
  return (
    <button className={`card news-card${featured ? " featured" : ""}`} onClick={() => api.openUrl(item.link)} title={item.link}>
      <Cover src={item.image} />
      <div className="news-body">
        <div className="news-kicker">
          {isNew && <span className="new-dot" />}
          {item.category && <span className="cat">{item.category}</span>}
          <span>{source}</span>
        </div>
        <h3 className="news-title">{item.title}</h3>
        {item.summary && <p className="news-summary">{item.summary}</p>}
        <div className="faint" style={{ fontSize: 11.5, marginTop: "auto", paddingTop: 4 }} title={formatDate(item.date, lang)}>
          {relativeTime(item.date, lang)}
          {item.author ? ` · ${item.author}` : ""}
        </div>
      </div>
    </button>
  );
}

export function NewsView() {
  const tr = useT();
  const lang = useStore((s) => s.lang);
  const { list, loading } = useStore((s) => s.news);
  const loadNews = useStore((s) => s.loadNews);
  const prevSeen = useStore((s) => s.prevSeen.news);
  const [source, setSource] = useState<Source>("all");

  const items = useMemo(
    () => (list?.items ?? []).filter((i) => source === "all" || i.source === source),
    [list, source],
  );

  return (
    <div className="view news">
      <div className="toolbar">
        <Segmented<Source>
          value={source}
          onChange={setSource}
          segments={[
            { value: "all", label: tr("news.all") },
            { value: "blender", label: tr("news.blender") },
            { value: "developers", label: tr("news.developers") },
          ]}
        />
        <div className="toolbar-status" style={{ marginLeft: "auto" }}>
          {!loading && list?.fetchedAt ? tr("builds.updated", { time: relativeTime(list.fetchedAt, lang) }) : null}
        </div>
        <button className="icon-btn" title={`${tr("common.refresh")} (Ctrl+R)`} onClick={() => loadNews(true)}>
          <RefreshCw size={16} className={loading ? "spin" : undefined} />
        </button>
      </div>

      {list?.error && (
        <div className="banner">
          <CloudOff size={15} />
          {tr("builds.offline")}
        </div>
      )}

      <div className="news-scroll">
        {!list && (
          <div className="placeholder">
            {loading ? <Loader2 size={26} className="spin" /> : <Newspaper size={34} strokeWidth={1.3} />}
            {loading ? null : tr("news.empty")}
          </div>
        )}
        <div className="news-grid">
          {items.map((item, i) => (
            <NewsCard key={item.id} item={item} featured={i === 0} isNew={item.date > prevSeen} />
          ))}
        </div>
      </div>
    </div>
  );
}
