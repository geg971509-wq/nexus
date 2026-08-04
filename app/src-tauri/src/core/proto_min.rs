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

/// ErrorResp { optional string error = 1 }
pub fn decode_error_resp(data: &[u8]) -> Option<String> {
    parse_string_field(data, 1)
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
}
