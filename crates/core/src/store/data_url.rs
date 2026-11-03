//! Images as base64 `data:` URLs, the form the panel and the model take.

/// `bytes` as a `data:` URL: how a captured file reaches the panel and the model.
pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = format!("data:{mime};base64,");
    out.reserve(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= c.len() {
                char::from(ABC[(n >> (18 - 6 * i) & 63) as usize])
            } else {
                '='
            });
        }
    }
    out
}

/// The bytes of a base64 `data:` URL, as [`data_url`] writes them: how a
/// stored image reaches the screen. None: not one.
pub fn data_url_bytes(url: &str) -> Option<Vec<u8>> {
    let (_, b64) = url.strip_prefix("data:")?.split_once(";base64,")?;
    let digit = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let b64 = b64.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(b64.len() / 4 * 3 + 2);
    for c in b64.chunks(4) {
        let n = c
            .iter()
            .try_fold(0u32, |n, b| Some(n << 6 | u32::from(digit(*b)?)))?;
        let n = n << (6 * (4 - c.len()));
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..c.len().saturating_sub(1)]);
    }
    Some(out)
}
