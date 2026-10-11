//! Reading and writing text files in the encoding they came in.
//!
//! Most Markdown is UTF-8, but older files (especially from Windows editors)
//! may be UTF-8 with a BOM, UTF-16, or Windows-1252. Decoding them properly
//! avoids "caf�" in the preview, and encoding back keeps saves faithful.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Windows1252,
}

impl Encoding {
    pub fn label(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8 with BOM",
            Encoding::Utf16Le => "UTF-16 LE",
            Encoding::Utf16Be => "UTF-16 BE",
            Encoding::Windows1252 => "Windows-1252",
        }
    }
}

/// Windows-1252 code points for bytes 0x80..=0x9F (the rest match Latin-1).
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
];

pub fn decode(bytes: &[u8]) -> (String, Encoding) {
    let (text, enc, _) = decode_checked(bytes);
    (text, enc)
}

/// Like [`decode`], also saying whether anything was lost (bytes that weren't
/// valid in the detected encoding). Saving such text would damage the file.
pub fn decode_checked(bytes: &[u8]) -> (String, Encoding, bool) {
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        let (text, lossy) = utf8(rest);
        return (text, Encoding::Utf8Bom, lossy);
    }
    if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        let (text, lossy) = utf16(rest, u16::from_le_bytes);
        return (text, Encoding::Utf16Le, lossy);
    }
    if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        let (text, lossy) = utf16(rest, u16::from_be_bytes);
        return (text, Encoding::Utf16Be, lossy);
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_owned(), Encoding::Utf8, false);
    }
    // Mostly UTF-8 with a stray bad byte: decoding it all as Windows-1252
    // would garble every accented letter, so keep UTF-8 and flag the damage.
    let (mut good, mut bad) = (0, 0);
    for chunk in bytes.utf8_chunks() {
        good += chunk.valid().chars().filter(|c| !c.is_ascii()).count();
        bad += usize::from(!chunk.invalid().is_empty());
    }
    if good > 0 && good >= bad {
        let (text, _) = utf8(bytes);
        return (text, Encoding::Utf8, true);
    }
    let text = bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => CP1252_HIGH[(b - 0x80) as usize],
            _ => b as char,
        })
        .collect();
    (text, Encoding::Windows1252, false)
}

fn utf8(bytes: &[u8]) -> (String, bool) {
    match std::str::from_utf8(bytes) {
        Ok(s) => (s.to_owned(), false),
        Err(_) => (String::from_utf8_lossy(bytes).into_owned(), true),
    }
}

fn utf16(bytes: &[u8], f: fn([u8; 2]) -> u16) -> (String, bool) {
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| f([c[0], c[1]])).collect();
    let odd = bytes.len() % 2 == 1;
    match String::from_utf16(&units) {
        Ok(s) => (s, odd),
        Err(_) => (String::from_utf16_lossy(&units), true),
    }
}

/// Encodes `text` back into `enc`. Returns `None` when a character can't be
/// represented (only possible for Windows-1252).
pub fn encode(text: &str, enc: Encoding) -> Option<Vec<u8>> {
    Some(match enc {
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Utf8Bom => [b"\xEF\xBB\xBF".as_slice(), text.as_bytes()].concat(),
        Encoding::Utf16Le => [0xFF, 0xFE]
            .into_iter()
            .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
            .collect(),
        Encoding::Utf16Be => [0xFE, 0xFF]
            .into_iter()
            .chain(text.encode_utf16().flat_map(u16::to_be_bytes))
            .collect(),
        Encoding::Windows1252 => {
            let mut out = Vec::with_capacity(text.len());
            for c in text.chars() {
                let b = match c as u32 {
                    n @ (0..=0x7F | 0xA0..=0xFF) => n as u8,
                    _ => CP1252_HIGH.iter().position(|&h| h == c)? as u8 + 0x80,
                };
                out.push(b);
            }
            out
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_encoding() {
        let text = "# Café — “quotes” €5\n";
        for enc in [
            Encoding::Utf8,
            Encoding::Utf8Bom,
            Encoding::Utf16Le,
            Encoding::Utf16Be,
            Encoding::Windows1252,
        ] {
            let bytes = encode(text, enc).unwrap();
            assert_eq!(decode(&bytes), (text.to_string(), enc), "{enc:?}");
        }
    }

    #[test]
    fn stray_bytes_in_utf8_are_flagged_not_garbled() {
        let (text, enc, lossy) = decode_checked(b"caf\xc3\xa9 \x92s\n");
        assert_eq!((enc, lossy), (Encoding::Utf8, true));
        assert!(text.starts_with("café "), "{text}");
        assert!(decode_checked(b"\xEF\xBB\xBFok\xff").2);
        assert!(decode_checked(b"\xFF\xFEa\x00b").2, "odd UTF-16 length");
        assert!(!decode_checked(b"plain").2);
    }

    #[test]
    fn detects_windows_1252() {
        assert_eq!(
            decode(b"caf\xe9 \x93hi\x94"),
            ("café “hi”".to_string(), Encoding::Windows1252)
        );
        assert!(encode("emoji 😀", Encoding::Windows1252).is_none());
    }
}
