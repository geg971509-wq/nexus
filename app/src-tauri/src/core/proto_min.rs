//! Minimal protobuf2 encode/decode for libcore smoke paths (no prost codegen yet).

pub fn encode_string_field1(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(2 + b.len() + 5);
    // field 1, wire type 2
    out.push(0x0a);
    write_varint(&mut out, b.len() as u64);
    out.extend_from_slice(b);
    out
}

pub fn encode_load_config_core_json(json: &str) -> Vec<u8> {
    encode_string_field1(json)
}

/// LoadConfigReq matching upstream C++ defaults (spb optional with = false always present).
/// Core Start derefs *NeedExtraProcess / *NeedXray — nil panics; always encode false bools.
/// Fields: core_config=1, disable_stats=2, need_extra_process=3, extra_no_out=8,
/// need_xray=9, profile_id=100.
pub fn encode_load_config_req(core_json: &str, profile_id: Option<i32>) -> Vec<u8> {
    let mut out = encode_string_field1(core_json);
    // bool fields upstream always materializes (default false)
    encode_bool_field(&mut out, 2, false); // disable_stats
    encode_bool_field(&mut out, 3, false); // need_extra_process
    encode_bool_field(&mut out, 8, false); // extra_no_out
    encode_bool_field(&mut out, 9, false); // need_xray
    // profile_id: upstream always sets; default -1 when None
    let pid = profile_id.unwrap_or(-1);
    write_varint(&mut out, (100u64 << 3) | 0);
    write_svarint32(&mut out, pid);
    out
}

fn encode_bool_field(out: &mut Vec<u8>, field: u32, v: bool) {
    write_varint(out, (u64::from(field) << 3) | 0);
    out.push(if v { 1 } else { 0 });
}

fn write_svarint32(out: &mut Vec<u8>, v: i32) {
    // protobuf int32 as varint of the 32-bit two's complement as u64
    write_varint(out, v as u32 as u64);
}


/// ErrorResp { optional string error = 1 }
pub fn decode_error_resp(data: &[u8]) -> Option<String> {
    parse_string_field(data, 1)
}

/// IsPrivilegedResponse { optional bool has_privilege = 1 }
pub fn decode_has_privilege(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i);
        i = ni;
        let field = (key >> 3) as u32;
        let wt = (key & 7) as u8;
        match (field, wt) {
            (1, 0) => {
                let (v, _ni) = read_varint(data, i);
                return v != 0;
            }
            (_, 0) => {
                let (_, ni) = read_varint(data, i);
                i = ni;
            }
            (_, 1) => i += 8,
            (_, 2) => {
                let (len, ni) = read_varint(data, i);
                i = ni + len as usize;
            }
            (_, 5) => i += 4,
            _ => break,
        }
    }
    false
}

/// CoreStateResponse { optional bool running = 1; optional int32 profile_id = 2 }
pub fn decode_core_state(data: &[u8]) -> (bool, i32) {
    let mut running = false;
    let mut profile_id = -1i32;
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i);
        i = ni;
        let field = (key >> 3) as u32;
        let wt = (key & 7) as u8;
        match (field, wt) {
            (1, 0) => {
                let (v, ni) = read_varint(data, i);
                i = ni;
                running = v != 0;
            }
            (2, 0) => {
                let (v, ni) = read_varint(data, i);
                i = ni;
                profile_id = v as i32;
            }
            (_, 0) => {
                let (_, ni) = read_varint(data, i);
                i = ni;
            }
            (_, 1) => i += 8,
            (_, 2) => {
                let (len, ni) = read_varint(data, i);
                i = ni + len as usize;
            }
            (_, 5) => i += 4,
            _ => break,
        }
    }
    (running, profile_id)
}

fn parse_string_field(data: &[u8], want: u32) -> Option<String> {
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i);
        i = ni;
        let field = (key >> 3) as u32;
        let wt = (key & 7) as u8;
        if wt == 2 {
            let (len, ni) = read_varint(data, i);
            i = ni;
            let end = i + len as usize;
            if end > data.len() {
                return None;
            }
            if field == want {
                return Some(String::from_utf8_lossy(&data[i..end]).into_owned());
            }
            i = end;
        } else if wt == 0 {
            let (_, ni) = read_varint(data, i);
            i = ni;
        } else {
            break;
        }
    }
    None
}

/// One live connection row for the UI table (subset of ConnectionMetaData).
#[derive(Debug, Clone, Default)]
pub struct ConnRow {
    pub id: String,
    /// Unix ms from Core `created_at` (0 if absent).
    pub created_at: i64,
    pub process: String,
    /// Full executable path when Core enricher resolved it (may be empty).
    pub process_path: String,
    /// OS process id when Core enricher resolved it (0 if unknown).
    pub process_id: u32,
    pub dest: String,
    pub domain: String,
    pub network: String,
    pub protocol: String,
    pub outbound: String,
    pub upload: i64,
    pub download: i64,
}

/// QueryConnectionsResp { repeated ConnectionMetaData active = 1; closed = 2 }.
/// Short HTTP(S) via mixed often lands in `closed` before the UI poll — include both
/// (active first). Cap closed so the table stays usable.
pub fn decode_query_connections(data: &[u8]) -> Vec<ConnRow> {
    let mut active = Vec::new();
    let mut closed = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i);
        i = ni;
        let field = (key >> 3) as u32;
        let wt = (key & 7) as u8;
        match (field, wt) {
            (1, 2) | (2, 2) => {
                let (len, ni) = read_varint(data, i);
                i = ni;
                let end = i + len as usize;
                if end > data.len() {
                    break;
                }
                let row = decode_conn_meta(&data[i..end]);
                if field == 1 {
                    active.push(row);
                } else {
                    closed.push(row);
                }
                i = end;
            }
            (_, 0) => {
                let (_, ni) = read_varint(data, i);
                i = ni;
            }
            (_, 1) => i += 8,
            (_, 2) => {
                let (len, ni) = read_varint(data, i);
                i = ni + len as usize;
            }
            (_, 5) => i += 4,
            _ => break,
        }
    }
    // ponytail: 80 closed cap; raise if UI needs longer history
    const MAX_CLOSED: usize = 80;
    if closed.len() > MAX_CLOSED {
        closed = closed.split_off(closed.len() - MAX_CLOSED);
    }
    active.extend(closed);
    active
}

fn decode_conn_meta(data: &[u8]) -> ConnRow {
    let mut row = ConnRow::default();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i);
        i = ni;
        let field = (key >> 3) as u32;
        let wt = (key & 7) as u8;
        match (field, wt) {
            (1, 2) => {
                // id
                if let Some((s, ni)) = read_len_str(data, i) {
                    row.id = s;
                    i = ni;
                } else {
                    break;
                }
            }
            (2, 0) => {
                // created_at (unix ms)
                let (v, ni) = read_varint(data, i);
                i = ni;
                row.created_at = v as i64;
            }
            (3, 0) => {
                let (v, ni) = read_varint(data, i);
                i = ni;
                row.upload = v as i64;
            }
            (4, 0) => {
                let (v, ni) = read_varint(data, i);
                i = ni;
                row.download = v as i64;
            }
            (5, 2) => {
                if let Some((s, ni)) = read_len_str(data, i) {
                    row.outbound = s;
                    i = ni;
                } else {
                    break;
                }
            }
            (6, 2) => {
                if let Some((s, ni)) = read_len_str(data, i) {
                    row.network = s;
                    i = ni;
                } else {
                    break;
                }
            }
            (7, 2) => {
                if let Some((s, ni)) = read_len_str(data, i) {
                    row.dest = s;
                    i = ni;
                } else {
                    break;
                }
            }
            (8, 2) => {
                if let Some((s, ni)) = read_len_str(data, i) {
                    row.protocol = s;
                    i = ni;
                } else {
                    break;
                }
            }
            (9, 2) => {
                if let Some((s, ni)) = read_len_str(data, i) {
                    row.domain = s;
                    i = ni;
                } else {
                    break;
                }
            }
            (10, 2) => {
                if let Some((s, ni)) = read_len_str(data, i) {
                    row.process = s;
                    i = ni;
                } else {
                    break;
                }
            }
            (11, 2) => {
                // process_path — full path for process-scoped block
                if let Some((s, ni)) = read_len_str(data, i) {
                    row.process_path = s;
                    i = ni;
                } else {
                    break;
                }
            }
            (14, 0) => {
                // process_id — OS PID
                let (v, ni) = read_varint(data, i);
                i = ni;
                row.process_id = v as u32;
            }
            (_, 0) => {
                let (_, ni) = read_varint(data, i);
                i = ni;
            }
            (_, 1) => i += 8,
            (_, 2) => {
                let (len, ni) = read_varint(data, i);
                i = ni + len as usize;
            }
            (_, 5) => i += 4,
            _ => break,
        }
    }
    row
}

fn read_len_str(data: &[u8], i: usize) -> Option<(String, usize)> {
    let (len, ni) = read_varint(data, i);
    let end = ni + len as usize;
    if end > data.len() {
        return None;
    }
    Some((String::from_utf8_lossy(&data[ni..end]).into_owned(), end))
}

/// QueryStatsResp: ups=1 / downs=2 map<string,int64> (protobuf map entries).
/// Returns cumulative outbound traffic for tag `proxy` (session tunnel).
pub fn decode_query_stats_proxy(data: &[u8]) -> (i64, i64) {
    let mut ups = std::collections::HashMap::<String, i64>::new();
    let mut downs = std::collections::HashMap::<String, i64>::new();
    let mut i = 0;
    while i < data.len() {
        let (key, ni) = read_varint(data, i);
        i = ni;
        let field = (key >> 3) as u32;
        let wt = (key & 7) as u8;
        match (field, wt) {
            (1, 2) | (2, 2) => {
                let (len, ni) = read_varint(data, i);
                i = ni;
                let end = i + len as usize;
                if end > data.len() {
                    break;
                }
                if let Some((k, v)) = decode_map_str_i64(&data[i..end]) {
                    if field == 1 {
                        ups.insert(k, v);
                    } else {
                        downs.insert(k, v);
                    }
                }
                i = end;
            }
            (_, 0) => {
                let (_, ni) = read_varint(data, i);
                i = ni;
            }
            (_, 1) => i += 8,
            (_, 2) => {
                let (len, ni) = read_varint(data, i);
                i = ni + len as usize;
            }
            (_, 5) => i += 4,
            _ => break,
        }
    }
    // Prefer explicit proxy tag; else sum non-direct outbounds (chain / rename edge cases).
    let pick = |m: &std::collections::HashMap<String, i64>| -> i64 {
        if let Some(v) = m.get("proxy") {
            return *v;
        }
        m.iter()
            .filter(|(k, _)| k.as_str() != "direct" && !k.starts_with("dns-"))
            .map(|(_, v)| *v)
            .sum()
    };
    (pick(&ups), pick(&downs))
}

fn decode_map_str_i64(data: &[u8]) -> Option<(String, i64)> {
    let mut key = String::new();
    let mut val: i64 = 0;
    let mut has_key = false;
    let mut i = 0;
    while i < data.len() {
        let (tag, ni) = read_varint(data, i);
        i = ni;
        let field = (tag >> 3) as u32;
        let wt = (tag & 7) as u8;
        match (field, wt) {
            (1, 2) => {
                let (s, ni) = read_len_str(data, i)?;
                key = s;
                has_key = true;
                i = ni;
            }
            (2, 0) => {
                let (v, ni) = read_varint(data, i);
                val = v as i64;
                i = ni;
            }
            (_, 0) => {
                let (_, ni) = read_varint(data, i);
                i = ni;
            }
            (_, 1) => i += 8,
            (_, 2) => {
                let (len, ni) = read_varint(data, i);
                i = ni + len as usize;
            }
            (_, 5) => i += 4,
            _ => break,
        }
    }
    if has_key {
        Some((key, val))
    } else {
        None
    }
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
            out.push(b);
        } else {
            out.push(b);
            break;
        }
    }
}

fn read_varint(data: &[u8], mut i: usize) -> (u64, usize) {
    let mut shift = 0u32;
    let mut out = 0u64;
    while i < data.len() {
        let b = data[i];
        i += 1;
        out |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return (out, i);
        }
        shift += 7;
        if shift > 63 {
            break;
        }
    }
    (out, i)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_string_field() {
        let enc = encode_string_field1("hi");
        assert_eq!(parse_string_field(&enc, 1).as_deref(), Some("hi"));
    }

    #[test]
    fn encode_load_has_bool_defaults() {
        let enc = encode_load_config_req("{}", Some(7));
        // must contain field 3 (need_extra_process) key = (3<<3)|0 = 0x18
        assert!(enc.windows(2).any(|w| w == [0x18, 0x00]), "need_extra_process=false missing: {enc:?}");
        // field 9 need_xray key = (9<<3)|0 = 0x48
        assert!(enc.windows(2).any(|w| w == [0x48, 0x00]), "need_xray=false missing: {enc:?}");
        assert_eq!(parse_string_field(&enc, 1).as_deref(), Some("{}"));
    }

    fn hand_meta(id: &str, process: &str) -> Vec<u8> {
        let mut meta = Vec::new();
        meta.push(0x0a);
        write_varint(&mut meta, id.len() as u64);
        meta.extend_from_slice(id.as_bytes());
        // created_at field 2 = 1_700_000_000_000 ms
        meta.push(0x10);
        write_varint(&mut meta, 1_700_000_000_000);
        meta.push(0x18);
        write_varint(&mut meta, 100);
        meta.push(0x20);
        write_varint(&mut meta, 200);
        meta.push(0x2a);
        write_varint(&mut meta, 5);
        meta.extend_from_slice(b"proxy");
        meta.push(0x32);
        write_varint(&mut meta, 3);
        meta.extend_from_slice(b"tcp");
        meta.push(0x3a);
        write_varint(&mut meta, 11);
        meta.extend_from_slice(b"1.1.1.1:443");
        meta.push(0x52);
        write_varint(&mut meta, process.len() as u64);
        meta.extend_from_slice(process.as_bytes());
        meta
    }

    #[test]
    fn decode_one_active_conn() {
        let meta = hand_meta("c1", "Chrome");
        let mut resp = Vec::new();
        resp.push(0x0a); // active field 1
        write_varint(&mut resp, meta.len() as u64);
        resp.extend_from_slice(&meta);
        let rows = decode_query_connections(&resp);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "c1");
        assert_eq!(rows[0].created_at, 1_700_000_000_000);
        assert_eq!(rows[0].process, "Chrome");
        assert_eq!(rows[0].dest, "1.1.1.1:443");
        assert_eq!(rows[0].outbound, "proxy");
        assert_eq!(rows[0].upload, 100);
        assert_eq!(rows[0].download, 200);
        assert_eq!(rows[0].network, "tcp");
    }

    #[test]
    fn decode_closed_conn_included() {
        // short curl finishes into closed=2 before UI poll
        let meta = hand_meta("c2", "curl");
        let mut resp = Vec::new();
        resp.push(0x12); // closed field 2
        write_varint(&mut resp, meta.len() as u64);
        resp.extend_from_slice(&meta);
        let rows = decode_query_connections(&resp);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "c2");
        assert_eq!(rows[0].process, "curl");
    }

    fn map_entry(k: &str, v: i64) -> Vec<u8> {
        let mut e = Vec::new();
        e.push(0x0a); // key field 1
        write_varint(&mut e, k.len() as u64);
        e.extend_from_slice(k.as_bytes());
        e.push(0x10); // value field 2
        write_varint(&mut e, v as u64);
        e
    }

    #[test]
    fn decode_query_stats_proxy_tag() {
        let mut resp = Vec::new();
        // ups: proxy=1000, direct=9
        for (k, v) in [("proxy", 1000i64), ("direct", 9)] {
            let e = map_entry(k, v);
            resp.push(0x0a);
            write_varint(&mut resp, e.len() as u64);
            resp.extend_from_slice(&e);
        }
        // downs: proxy=2000
        let e = map_entry("proxy", 2000);
        resp.push(0x12);
        write_varint(&mut resp, e.len() as u64);
        resp.extend_from_slice(&e);
        let (u, d) = decode_query_stats_proxy(&resp);
        assert_eq!(u, 1000);
        assert_eq!(d, 2000);
    }
}
