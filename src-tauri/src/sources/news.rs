//! News from the official blender.org and developer blog RSS feeds.

use crate::error::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NewsItem {
    pub id: String,
    pub source: String,
    pub title: String,
    pub link: String,
    pub date: i64,
    pub summary: String,
    pub image: Option<String>,
    pub category: Option<String>,
    pub author: Option<String>,
}

pub const FEEDS: &[(&str, &str)] = &[
    ("blender", "https://www.blender.org/feed/"),
    ("developers", "https://code.blender.org/feed/"),
];

static ITEM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<item>(.*?)</item>").unwrap());
static IMG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)<img[^>]+?src=["']([^"']+)["']"#).unwrap());
static ENCLOSURE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)<(?:enclosure|media:content|media:thumbnail)[^>]+url=["']([^"']+)["']"#)
        .unwrap()
});
static TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]*>").unwrap());
static SPACE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static ENTITY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&(#[0-9]+|#[xX][0-9a-fA-F]+|[a-zA-Z]+);").unwrap());

pub async fn fetch_all(client: &reqwest::Client) -> Result<Vec<NewsItem>> {
    let results = futures_util::future::join_all(
        FEEDS
            .iter()
            .map(|(source, url)| fetch_feed(client, source, url)),
    )
    .await;
    let mut items = Vec::new();
    let mut first_error = None;
    for r in results {
        match r {
            Ok(mut v) => items.append(&mut v),
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    if items.is_empty() {
        if let Some(e) = first_error {
            return Err(e);
        }
    }
    items.sort_by_key(|i| std::cmp::Reverse(i.date));
    Ok(items)
}

async fn fetch_feed(client: &reqwest::Client, source: &str, url: &str) -> Result<Vec<NewsItem>> {
    let xml = super::get(client, url).await?.text().await?;
    Ok(parse_feed(&xml, source))
}

fn parse_feed(xml: &str, source: &str) -> Vec<NewsItem> {
    ITEM_RE
        .captures_iter(xml)
        .filter_map(|c| {
            let item = &c[1];
            let title = decode_entities(&strip_cdata(&tag(item, "title")?));
            let link = strip_cdata(&tag(item, "link")?).trim().to_string();
            let date = tag(item, "pubDate")
                .and_then(|d| chrono::DateTime::parse_from_rfc2822(d.trim()).ok())
                .map(|d| d.timestamp())
                .unwrap_or(0);
            let content = tag(item, "content:encoded")
                .map(|c| strip_cdata(&c))
                .unwrap_or_default();
            let description = tag(item, "description")
                .map(|d| strip_cdata(&d))
                .unwrap_or_default();
            let image = ENCLOSURE_RE
                .captures(item)
                .or_else(|| IMG_RE.captures(&content))
                .or_else(|| IMG_RE.captures(&description))
                .map(|m| decode_entities(&m[1]))
                .filter(|u| u.starts_with("https://"));
            let summary_src = if description.trim().is_empty() {
                &content
            } else {
                &description
            };
            Some(NewsItem {
                id: tag(item, "guid")
                    .map(|g| strip_cdata(&g))
                    .unwrap_or_else(|| link.clone()),
                source: source.to_string(),
                summary: truncate(&plain_text(summary_src), 280),
                category: tag(item, "category").map(|c| decode_entities(&strip_cdata(&c))),
                author: tag(item, "dc:creator").map(|c| decode_entities(&strip_cdata(&c))),
                title,
                link,
                date,
                image,
            })
        })
        .collect()
}

/// Text content of the first `<name>` element (attributes allowed).
fn tag(item: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let mut search = 0;
    while let Some(rel) = item[search..].find(&open) {
        let start = search + rel;
        let after = start + open.len();
        // Make sure `<category` does not match `<categoryfoo`.
        match item[after..].chars().next() {
            Some('>') | Some(' ') | Some('\t') | Some('\n') | Some('\r') => {}
            _ => {
                search = after;
                continue;
            }
        }
        let body_start = after + item[after..].find('>')? + 1;
        let body_end = body_start + item[body_start..].find(&close)?;
        return Some(item[body_start..body_end].to_string());
    }
    None
}

fn strip_cdata(s: &str) -> String {
    let t = s.trim();
    t.strip_prefix("<![CDATA[")
        .and_then(|r| r.strip_suffix("]]>"))
        .unwrap_or(t)
        .to_string()
}

fn plain_text(html: &str) -> String {
    let text = TAG_RE.replace_all(html, " ");
    let text = decode_entities(&text);
    SPACE_RE.replace_all(text.trim(), " ").into_owned()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    let cut = cut.rsplit_once(' ').map(|(a, _)| a).unwrap_or(&cut);
    format!(
        "{}…",
        cut.trim_end_matches(|c: char| c.is_ascii_punctuation())
    )
}

pub fn decode_entities(s: &str) -> String {
    ENTITY_RE
        .replace_all(s, |c: &regex::Captures| {
            let e = &c[1];
            let ch = if let Some(hex) = e.strip_prefix("#x").or_else(|| e.strip_prefix("#X")) {
                u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
            } else if let Some(dec) = e.strip_prefix('#') {
                dec.parse().ok().and_then(char::from_u32)
            } else {
                match e {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    "rsquo" => Some('’'),
                    "lsquo" => Some('‘'),
                    "rdquo" => Some('”'),
                    "ldquo" => Some('“'),
                    "hellip" => Some('…'),
                    "ndash" => Some('–'),
                    "mdash" => Some('—'),
                    "times" => Some('×'),
                    _ => None,
                }
            };
            ch.map(String::from).unwrap_or_else(|| c[0].to_string())
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wordpress_item() {
        let xml = r#"<rss><channel><item>
		<title>Blender 5.1 &#8211; Released</title>
		<link>https://www.blender.org/news/x/</link>
		<dc:creator><![CDATA[Ton]]></dc:creator>
		<pubDate>Tue, 13 Oct 2026 14:35:20 +0000</pubDate>
		<category><![CDATA[Releases]]></category>
		<guid isPermaLink="false">https://www.blender.org/?p=1</guid>
		<description><![CDATA[Hello &amp; welcome.]]></description>
		<content:encoded><![CDATA[<p>x</p><img width="1" src="https://www.blender.org/a.jpg" />]]></content:encoded>
		</item></channel></rss>"#;
        let items = parse_feed(xml, "blender");
        assert_eq!(items.len(), 1);
        let i = &items[0];
        assert_eq!(i.title, "Blender 5.1 – Released");
        assert_eq!(i.summary, "Hello & welcome.");
        assert_eq!(i.image.as_deref(), Some("https://www.blender.org/a.jpg"));
        assert_eq!(i.category.as_deref(), Some("Releases"));
        assert!(i.date > 0);
    }
}
