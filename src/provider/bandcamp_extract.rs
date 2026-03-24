use anyhow::Result;
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

use crate::provider::{MediaKind, ProviderKind, ResolvedMedia, SearchItem};

pub fn parse_search_results(html: &str) -> Result<Vec<SearchItem>> {
    let document = Html::parse_document(html);

    let root_selectors = selectors(&[
        "li.searchresult",
        ".result-items li.searchresult",
        ".result-items > li",
        "ul.result-items > li",
    ])?;

    let heading_link_selectors = selectors(&[
        ".heading a",
        ".result-info .heading a",
        "a.itemurl",
        "a[href]",
    ])?;

    let heading_text_selectors = selectors(&[".heading", ".result-info .heading", ".itemtext"])?;

    let subhead_selectors = selectors(&[".subhead", ".result-info .subhead", ".itemsubtext"])?;

    let type_selectors = selectors(&[".itemtype", ".result-type", ".type"])?;

    let mut items = Vec::new();

    for root_selector in &root_selectors {
        for node in document.select(root_selector) {
            if let Some(item) = extract_item(
                node,
                &heading_link_selectors,
                &heading_text_selectors,
                &subhead_selectors,
                &type_selectors,
            ) {
                items.push(item);
            }
        }

        if !items.is_empty() {
            break;
        }
    }

    Ok(dedup_items(items))
}

pub fn parse_release_page(page_url: &str, html: &str) -> Result<ResolvedMedia> {
    let title = extract_meta_content(html, "og:title")
        .or_else(|| extract_html_title(html))
        .unwrap_or_else(|| "unknown".to_string());

    let artist = extract_artist_from_ld_json(html)
        .or_else(|| extract_meta_content(html, "og:site_name"))
        .filter(|s| !s.trim().is_empty());

    let preview_url = extract_preview_url(html);

    let kind = detect_kind_from_page_url(page_url);

    Ok(ResolvedMedia {
        provider: ProviderKind::Bandcamp,
        kind,
        title: clean_text(&title),
        artist: artist.map(|s| clean_text(&s)),
        page_url: page_url.to_string(),
        playable: preview_url.is_some(),
        preview_url,
    })
}

fn extract_item(
    node: ElementRef<'_>,
    heading_link_selectors: &[Selector],
    heading_text_selectors: &[Selector],
    subhead_selectors: &[Selector],
    type_selectors: &[Selector],
) -> Option<SearchItem> {
    let url = first_attr(&node, heading_link_selectors, "href")?;
    let title = first_text(&node, heading_link_selectors)
        .or_else(|| first_text(&node, heading_text_selectors))?;
    let title = clean_text(&title);

    if title.is_empty() {
        return None;
    }

    let kind_text = first_text(&node, type_selectors)
        .map(|s| clean_text(&s))
        .unwrap_or_default();

    let kind = detect_media_kind(&kind_text, &url, &title);

    let subhead_text = first_text(&node, subhead_selectors)
        .map(|s| clean_text(&s))
        .unwrap_or_default();

    let artist = extract_artist(&subhead_text, &title);

    Some(SearchItem {
        provider: ProviderKind::Bandcamp,
        kind,
        title,
        artist,
        url: absolutize_url(&url),
        playable: false,
        preview_url: None,
    })
}

fn selectors(patterns: &[&str]) -> Result<Vec<Selector>> {
    patterns
        .iter()
        .map(|p| {
            Selector::parse(p).map_err(|err| anyhow::anyhow!("invalid selector: {p}: {err:?}"))
        })
        .collect()
}

fn first_text(node: &ElementRef<'_>, selectors: &[Selector]) -> Option<String> {
    selectors.iter().find_map(|selector| {
        node.select(selector)
            .next()
            .map(collect_text)
            .filter(|text| !clean_text(text).is_empty())
    })
}

fn first_attr(node: &ElementRef<'_>, selectors: &[Selector], attr: &str) -> Option<String> {
    selectors.iter().find_map(|selector| {
        node.select(selector)
            .next()
            .and_then(|el| el.value().attr(attr))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

fn collect_text(node: ElementRef<'_>) -> String {
    node.text().collect::<Vec<_>>().join(" ")
}

fn clean_text(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn detect_media_kind(kind_text: &str, url: &str, title: &str) -> MediaKind {
    let k = kind_text.to_lowercase();
    let u = url.to_lowercase();
    let t = title.to_lowercase();

    if k.contains("artist") || u.contains("/music") || t.starts_with("artist:") {
        MediaKind::Artist
    } else if k.contains("track") || u.contains("/track/") || t.starts_with("track:") {
        MediaKind::Track
    } else {
        MediaKind::Album
    }
}

fn detect_kind_from_page_url(url: &str) -> MediaKind {
    let normalized = url.to_lowercase();

    if normalized.contains("/track/") {
        MediaKind::Track
    } else if normalized.contains("/music") {
        MediaKind::Artist
    } else {
        MediaKind::Album
    }
}

fn extract_artist(subhead: &str, title: &str) -> Option<String> {
    let lowered = subhead.to_lowercase();

    let prefixes = ["by ", "from ", "artist: ", "album by ", "track by "];

    for prefix in prefixes {
        if lowered.starts_with(prefix) && subhead.len() >= prefix.len() {
            let value = clean_text(&subhead[prefix.len()..]);
            if !value.is_empty() {
                return Some(value);
            }
        }
    }

    if let Some((left, right)) = title.split_once(" - ") {
        let left = clean_text(left);
        let right = clean_text(right);

        if !left.is_empty() && !right.is_empty() {
            return Some(left);
        }
    }

    None
}

fn absolutize_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if url.starts_with("//") {
        format!("https:{url}")
    } else if url.starts_with('/') {
        format!("https://bandcamp.com{url}")
    } else {
        url.to_string()
    }
}

fn dedup_items(items: Vec<SearchItem>) -> Vec<SearchItem> {
    let mut out = Vec::new();

    for item in items {
        let exists = out.iter().any(|existing: &SearchItem| {
            existing.url == item.url
                && existing.title == item.title
                && existing.artist == item.artist
                && existing.kind == item.kind
        });

        if !exists {
            out.push(item);
        }
    }

    out
}

fn extract_meta_content(html: &str, property_name: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("meta").ok()?;

    for meta in document.select(&selector) {
        let value = meta.value();
        let property = value.attr("property").or_else(|| value.attr("name"));

        if property == Some(property_name) {
            if let Some(content) = value.attr("content") {
                let cleaned = clean_text(content);
                if !cleaned.is_empty() {
                    return Some(cleaned);
                }
            }
        }
    }

    None
}

fn extract_html_title(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("title").ok()?;

    document
        .select(&selector)
        .next()
        .map(collect_text)
        .map(|s| clean_text(&s))
        .filter(|s| !s.is_empty())
}

fn extract_artist_from_ld_json(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse(r#"script[type="application/ld+json"]"#).ok()?;

    for node in document.select(&selector) {
        let raw = collect_text(node);
        let Ok(json) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };

        if let Some(name) = json
            .get("byArtist")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
        {
            let cleaned = clean_text(name);
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }

        if let Some(name) = json
            .get("author")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
        {
            let cleaned = clean_text(name);
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }

    None
}

fn extract_preview_url(html: &str) -> Option<String> {
    let keys = [
        "\"mp3-128\":\"",
        "\"mp3-128\": \"",
        "&quot;mp3-128&quot;:&quot;",
        "&quot;mp3-128&quot;: &quot;",
    ];

    for key in keys {
        if let Some(start) = html.find(key) {
            let value_start = start + key.len();
            let rest = &html[value_start..];

            let terminator = if key.contains("&quot;") {
                "&quot;"
            } else {
                "\""
            };

            if let Some(end) = rest.find(terminator) {
                let raw = &rest[..end];
                let decoded = raw.replace("\\/", "/").replace("&amp;", "&");
                let cleaned = decoded.trim().to_string();

                if cleaned.starts_with("http://") || cleaned.starts_with("https://") {
                    return Some(cleaned);
                }
            }
        }
    }

    None
}
