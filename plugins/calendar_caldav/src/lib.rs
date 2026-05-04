mod sdk;
use serde_json::{json, Map, Value};

#[no_mangle]
pub extern "C" fn taskhub_alloc(size: u32) -> u32 { sdk::alloc(size) }
#[no_mangle]
pub extern "C" fn taskhub_dealloc(ptr: u32, size: u32) { sdk::dealloc(ptr, size) }
#[no_mangle]
pub extern "C" fn taskhub_execute(ap: u32, al: u32, ip: u32, il: u32) -> u64 {
    sdk::execute_action(ap, al, ip, il, dispatch)
}

fn basic_auth(user: &str, pass: &str) -> String {
    // base64 encode user:pass (inline, no dep)
    let raw = format!("{}:{}", user, pass);
    b64_encode(raw.as_bytes())
}

fn caldav_request(method: &str, url: &str, user: &str, pass: &str, content_type: &str, body: &str) -> Option<String> {
    let mut h = Map::new();
    h.insert("Authorization".into(), Value::String(format!("Basic {}", basic_auth(user, pass))));
    h.insert("Content-Type".into(), Value::String(content_type.to_string()));
    h.insert("Depth".into(), Value::String("1".into()));
    let body_bytes = body.as_bytes();
    sdk::http_request(method, url, Some(&h), if body.is_empty() { None } else { Some(body_bytes) })
        .and_then(|b| String::from_utf8(b).ok())
}

fn dispatch(action: &str, input: Value) -> Value {
    let url = input["url"].as_str().unwrap_or("");
    let user = input["username"].as_str().unwrap_or("");
    let pass = input["password"].as_str().unwrap_or("");
    if url.is_empty() || user.is_empty() { return sdk::respond_error("missing url or username"); }

    match action {
        "events.list" => {
            let start = input["start_iso"].as_str().unwrap_or("");
            let end = input["end_iso"].as_str().unwrap_or("");
            if start.is_empty() || end.is_empty() { return sdk::respond_error("missing start_iso or end_iso"); }

            let report_body = format!(r#"<?xml version="1.0" encoding="utf-8" ?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop><D:getetag/><C:calendar-data/></D:prop>
  <C:filter>
    <C:comp-filter name="VCALENDAR">
      <C:comp-filter name="VEVENT">
        <C:time-range start="{}" end="{}"/>
      </C:comp-filter>
    </C:comp-filter>
  </C:filter>
</C:calendar-query>"#, to_ical_dt(start), to_ical_dt(end));

            match caldav_request("REPORT", url, user, pass, "application/xml", &report_body) {
                Some(xml) => {
                    let events = parse_ical_events(&xml);
                    json!(events)
                }
                None => sdk::respond_error("events.list request failed"),
            }
        }
        "events.create" => {
            let summary = input["summary"].as_str().unwrap_or("Untitled");
            let dtstart = input["dtstart"].as_str().unwrap_or("");
            let dtend = input["dtend"].as_str().unwrap_or("");
            if dtstart.is_empty() || dtend.is_empty() { return sdk::respond_error("missing dtstart or dtend"); }

            let uid = gen_uid();
            let mut ical = format!(
                "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//TaskHub//CalDAV//EN\r\nBEGIN:VEVENT\r\nUID:{}\r\nSUMMARY:{}\r\nDTSTART:{}\r\nDTEND:{}\r\n",
                uid, summary, to_ical_dt(dtstart), to_ical_dt(dtend)
            );
            if let Some(d) = input["description"].as_str() {
                ical.push_str(&format!("DESCRIPTION:{}\r\n", d));
            }
            if let Some(l) = input["location"].as_str() {
                ical.push_str(&format!("LOCATION:{}\r\n", l));
            }
            ical.push_str("END:VEVENT\r\nEND:VCALENDAR\r\n");

            let event_url = format!("{}/{}.ics", url.trim_end_matches('/'), uid);
            match caldav_request("PUT", &event_url, user, pass, "text/calendar; charset=utf-8", &ical) {
                Some(_) => json!({"uid": uid, "url": event_url}),
                None => sdk::respond_error("events.create failed"),
            }
        }
        _ => sdk::respond_error(&format!("unknown action: {}", action)),
    }
}

fn to_ical_dt(iso: &str) -> String {
    // 2024-01-15T10:00:00Z → 20240115T100000Z
    iso.replace('-', "").replace(':', "").replace(' ', "T")
}

fn parse_ical_events(xml: &str) -> Vec<Value> {
    let mut results = vec![];
    let mut rest = xml;
    while let Some(start) = rest.find("BEGIN:VEVENT") {
        let end_tag = "END:VEVENT";
        let end = match rest[start..].find(end_tag) {
            Some(i) => start + i + end_tag.len(),
            None => break,
        };
        let block = &rest[start..end];
        let uid = ical_field(block, "UID").unwrap_or_default();
        let summary = ical_field(block, "SUMMARY").unwrap_or_default();
        let dtstart = ical_field(block, "DTSTART").unwrap_or_default();
        let dtend = ical_field(block, "DTEND").unwrap_or_default();
        let location = ical_field(block, "LOCATION");
        let mut ev = json!({"uid": uid, "summary": summary, "dtstart": dtstart, "dtend": dtend});
        if let Some(l) = location { ev["location"] = json!(l); }
        results.push(ev);
        rest = &rest[end..];
    }
    results
}

fn ical_field(block: &str, name: &str) -> Option<String> {
    let prefix = format!("{}:", name);
    for line in block.lines() {
        if line.starts_with(&prefix) {
            return Some(line[prefix.len()..].trim().to_string());
        }
        // param form: DTSTART;TZID=...
        if line.starts_with(name) && line.contains(':') {
            let v = line.splitn(2, ':').nth(1)?;
            return Some(v.trim().to_string());
        }
    }
    None
}

fn gen_uid() -> String {
    // Simple UID based on current "random" state; good enough for CalDAV.
    format!("taskhub-{:x}", pseudo_rand())
}

fn pseudo_rand() -> u64 {
    // Cheap entropy: use pointer address of a stack var as seed.
    let x: u64 = 0;
    let ptr = &x as *const u64 as u64;
    ptr.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}

const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(data: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < data.len() {
        let b0 = data[i] as u32;
        let b1 = if i + 1 < data.len() { data[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < data.len() { data[i + 2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64[((n >> 18) & 63) as usize] as char);
        out.push(B64[((n >> 12) & 63) as usize] as char);
        if i + 1 < data.len() { out.push(B64[((n >> 6) & 63) as usize] as char); } else { out.push('='); }
        if i + 2 < data.len() { out.push(B64[(n & 63) as usize] as char); } else { out.push('='); }
        i += 3;
    }
    out
}
