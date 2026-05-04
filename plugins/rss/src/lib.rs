mod sdk;
use serde_json::{json, Value};

#[no_mangle]
pub extern "C" fn taskhub_alloc(size: u32) -> u32 { sdk::alloc(size) }
#[no_mangle]
pub extern "C" fn taskhub_dealloc(ptr: u32, size: u32) { sdk::dealloc(ptr, size) }
#[no_mangle]
pub extern "C" fn taskhub_execute(ap: u32, al: u32, ip: u32, il: u32) -> u64 {
    sdk::execute_action(ap, al, ip, il, dispatch)
}

fn dispatch(action: &str, input: Value) -> Value {
    let url = input["url"].as_str().unwrap_or("");
    if url.is_empty() { return sdk::respond_error("missing url"); }
    let limit = input["limit"].as_u64().unwrap_or(20) as usize;

    let xml = match sdk::http_get(url) {
        Some(s) => s,
        None => return sdk::respond_error("failed to fetch feed"),
    };

    // Hardened minimal XML parser — no DOCTYPE, no external entities.
    if xml.contains("<!DOCTYPE") || xml.contains("<!ENTITY") {
        return sdk::respond_error("feed contains forbidden DOCTYPE/ENTITY");
    }
    if xml.len() > 5 * 1024 * 1024 {
        return sdk::respond_error("feed too large (>5MB)");
    }

    let entries = parse_feed(&xml, limit);

    match action {
        "fetch" => json!(entries),
        "check_new" => {
            let since = input["since_iso"].as_str().unwrap_or("");
            if since.is_empty() { return sdk::respond_error("missing since_iso"); }
            let newer: Vec<_> = entries.into_iter().filter(|e| {
                e["published"].as_str().map(|p| p > since).unwrap_or(false)
            }).collect();
            json!(newer)
        }
        _ => sdk::respond_error(&format!("unknown action: {}", action)),
    }
}

fn parse_feed(xml: &str, limit: usize) -> Vec<Value> {
    let is_atom = xml.contains("<feed");
    if is_atom {
        parse_atom(xml, limit)
    } else {
        parse_rss(xml, limit)
    }
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim().to_string())
}

fn extract_cdata(s: &str) -> String {
    if s.starts_with("<![CDATA[") && s.ends_with("]]>") {
        s[9..s.len()-3].to_string()
    } else {
        s.to_string()
    }
}

fn extract_attr(tag_str: &str, attr: &str) -> Option<String> {
    let needle = format!("{}=\"", attr);
    let start = tag_str.find(&needle)? + needle.len();
    let end = tag_str[start..].find('"')? + start;
    Some(tag_str[start..end].to_string())
}

fn parse_rss(xml: &str, limit: usize) -> Vec<Value> {
    let mut results = vec![];
    let mut rest = xml;
    while let Some(item_start) = rest.find("<item") {
        if results.len() >= limit { break; }
        let inner_start = match rest[item_start..].find('>') {
            Some(i) => item_start + i + 1,
            None => break,
        };
        let inner_end = match rest[inner_start..].find("</item>") {
            Some(i) => inner_start + i,
            None => break,
        };
        let item = &rest[inner_start..inner_end];
        let title = extract_tag(item, "title").map(|s| extract_cdata(&s)).unwrap_or_default();
        let link = extract_tag(item, "link").map(|s| extract_cdata(&s)).unwrap_or_default();
        let published = extract_tag(item, "pubDate").or_else(|| extract_tag(item, "dc:date")).unwrap_or_default();
        let summary = extract_tag(item, "description").map(|s| extract_cdata(&s)).unwrap_or_default();
        results.push(json!({"title": title, "link": link, "published": published, "summary": summary}));
        rest = &rest[inner_end + 7..];
    }
    results
}

fn parse_atom(xml: &str, limit: usize) -> Vec<Value> {
    let mut results = vec![];
    let mut rest = xml;
    while let Some(entry_pos) = rest.find("<entry") {
        if results.len() >= limit { break; }
        let inner_start = match rest[entry_pos..].find('>') {
            Some(i) => entry_pos + i + 1,
            None => break,
        };
        let inner_end = match rest[inner_start..].find("</entry>") {
            Some(i) => inner_start + i,
            None => break,
        };
        let entry = &rest[inner_start..inner_end];
        let title = extract_tag(entry, "title").map(|s| extract_cdata(&s)).unwrap_or_default();
        let published = extract_tag(entry, "published").or_else(|| extract_tag(entry, "updated")).unwrap_or_default();
        let summary = extract_tag(entry, "summary").or_else(|| extract_tag(entry, "content")).map(|s| extract_cdata(&s)).unwrap_or_default();
        // atom link: <link href="..." rel="alternate"/>
        let link = rest[entry_pos..inner_end].find("<link ")
            .and_then(|p| extract_attr(&rest[entry_pos + p..entry_pos + p + 200], "href"))
            .unwrap_or_default();
        results.push(json!({"title": title, "link": link, "published": published, "summary": summary}));
        rest = &rest[inner_end + 8..];
    }
    results
}
