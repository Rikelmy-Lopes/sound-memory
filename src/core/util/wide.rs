//! UTF-16 helpers for Win32 calls.

/// UTF-16, NUL-terminated buffer for Win32 calls.
pub fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copies a Rust string into a fixed-size Win32 wide buffer, NUL-terminating it.
pub fn set_wide(dst: &mut [u16], value: &str) {
    if dst.is_empty() {
        return;
    }
    let mut len = 0;
    for ch in value.chars() {
        let mut buf = [0u16; 2];
        let encoded = ch.encode_utf16(&mut buf);
        if len + encoded.len() >= dst.len() {
            break;
        }
        dst[len..len + encoded.len()].copy_from_slice(encoded);
        len += encoded.len();
    }
    dst[len] = 0;
}

pub fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_keeps_short_strings() {
        assert_eq!(truncate("Headset", 10), "Headset");
        assert_eq!(truncate("Headset", 7), "Headset");
    }

    #[test]
    fn truncate_counts_characters_not_bytes() {
        assert_eq!(truncate("联想耳机", 3), "联想…");
        assert_eq!(truncate("联想耳机", 4), "联想耳机");
    }

    #[test]
    fn wide_strings_are_nul_terminated() {
        let wide = to_wide("ab");
        assert_eq!(wide, vec![97, 98, 0]);
    }

    #[test]
    fn fixed_buffers_are_not_overrun() {
        let mut buffer = [0u16; 4];
        set_wide(&mut buffer, "abcd");
        // Room for three UTF-16 units plus the terminator.
        assert_eq!(buffer, [97, 98, 99, 0]);

        let mut buffer = [0u16; 3];
        set_wide(&mut buffer, "联想耳机");
        assert_eq!(buffer, [0x8054, 0x60f3, 0]);
    }

    #[test]
    fn surrogate_pairs_are_never_split() {
        let mut buffer = [0u16; 4];
        set_wide(&mut buffer, "😀😀");
        // Each emoji is two UTF-16 units, so only one fits beside the terminator.
        assert_eq!(buffer[0..3], [0xD83D, 0xDE00, 0]);
    }
}
