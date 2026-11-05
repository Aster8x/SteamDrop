//! Steam's binary KeyValues format, as used by `shortcuts.vdf`.
//! Entries keep their order and any fields we don't know about, so a
//! read-then-write round trip changes nothing we didn't touch.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Map(Map),
    Str(String),
    Int(u32),
    Float(u32),
    U64(u64),
}

pub type Map = Vec<(String, Value)>;

const MAP: u8 = 0;
const STR: u8 = 1;
const INT: u8 = 2;
const FLOAT: u8 = 3;
const U64: u8 = 7;
const END: u8 = 8;

pub fn parse(bytes: &[u8]) -> Result<Map, String> {
    let mut pos = 0;
    let map = parse_map(bytes, &mut pos, true)?;
    Ok(map)
}

fn parse_map(b: &[u8], pos: &mut usize, top: bool) -> Result<Map, String> {
    let mut map = Map::new();
    loop {
        let Some(&kind) = b.get(*pos) else {
            return if top { Ok(map) } else { Err("file ends inside a section".into()) };
        };
        *pos += 1;
        if kind == END {
            return Ok(map);
        }
        let key = read_str(b, pos)?;
        let value = match kind {
            MAP => Value::Map(parse_map(b, pos, false)?),
            STR => Value::Str(read_str(b, pos)?),
            INT => Value::Int(u32::from_le_bytes(take(b, pos)?)),
            FLOAT => Value::Float(u32::from_le_bytes(take(b, pos)?)),
            U64 => Value::U64(u64::from_le_bytes(take(b, pos)?)),
            // Refuse to guess: writing back a file we only half understood could lose shortcuts.
            other => return Err(format!("unknown field type {other} at byte {}", *pos)),
        };
        map.push((key, value));
    }
}

fn take<const N: usize>(b: &[u8], pos: &mut usize) -> Result<[u8; N], String> {
    let s = b.get(*pos..*pos + N).ok_or("file ends inside a number")?;
    *pos += N;
    Ok(s.try_into().unwrap())
}

fn read_str(b: &[u8], pos: &mut usize) -> Result<String, String> {
    let rest = &b[*pos..];
    let len = rest.iter().position(|&c| c == 0).ok_or("file ends inside a text field")?;
    *pos += len + 1;
    Ok(String::from_utf8_lossy(&rest[..len]).into_owned())
}

pub fn write(map: &Map) -> Vec<u8> {
    let mut out = Vec::new();
    write_map(map, &mut out);
    out.push(END);
    out
}

fn write_map(map: &Map, out: &mut Vec<u8>) {
    for (key, value) in map {
        let kind = match value {
            Value::Map(_) => MAP,
            Value::Str(_) => STR,
            Value::Int(_) => INT,
            Value::Float(_) => FLOAT,
            Value::U64(_) => U64,
        };
        out.push(kind);
        out.extend_from_slice(key.as_bytes());
        out.push(0);
        match value {
            Value::Map(m) => {
                write_map(m, out);
                out.push(END);
            }
            Value::Str(s) => {
                out.extend_from_slice(s.as_bytes());
                out.push(0);
            }
            Value::Int(n) | Value::Float(n) => out.extend_from_slice(&n.to_le_bytes()),
            Value::U64(n) => out.extend_from_slice(&n.to_le_bytes()),
        }
    }
}

/// Steam is inconsistent about key case ("AppName" vs "appname"), so lookups ignore it.
pub fn get<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
}

pub fn get_int(map: &Map, key: &str) -> Option<u32> {
    match get(map, key)? {
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_everything() {
        let map: Map = vec![(
            "shortcuts".into(),
            Value::Map(vec![(
                "0".into(),
                Value::Map(vec![
                    ("appid".into(), Value::Int(0x8123_4567)),
                    ("AppName".into(), Value::Str("Gämé".into())),
                    ("LastPlayTime".into(), Value::Int(1_700_000_000)),
                    ("weird".into(), Value::U64(42)),
                    ("tags".into(), Value::Map(vec![("0".into(), Value::Str("x".into()))])),
                ]),
            )]),
        )];
        let bytes = write(&map);
        assert_eq!(&bytes[..11], b"\x00shortcuts\x00");
        assert_eq!(&bytes[bytes.len() - 3..], &[END, END, END]);
        assert_eq!(parse(&bytes).unwrap(), map);
        assert_eq!(write(&parse(&bytes).unwrap()), bytes);
    }

    #[test]
    fn rejects_broken_files() {
        assert!(parse(b"\x00shortcuts\x00\x01name").is_err());
        assert!(parse(b"\x00shortcuts\x00\x09x\x00").is_err());
        assert_eq!(parse(b"").unwrap(), Map::new());
    }

    #[test]
    fn keys_ignore_case() {
        let m: Map = vec![("appname".into(), Value::Str("A".into()))];
        assert_eq!(get(&m, "AppName"), Some(&Value::Str("A".into())));
    }
}
