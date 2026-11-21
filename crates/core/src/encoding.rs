//! Text encodings the app writes by hand (RFC 3986, RFC 4648): small enough
//! not to need a dependency, and in one place so there is one copy.

use std::fmt::Write as _;

/// Percent-encodes `text` for a URL query value: everything but the RFC 3986
/// unreserved characters.
#[must_use]
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Standard base64 with padding (RFC 4648 §4), for audio in a JSON body.
#[must_use]
pub fn base64(bytes: &[u8]) -> String {
    encode(
        bytes,
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/",
        true,
    )
}

/// Base64url without padding (RFC 4648 §5), for PKCE.
#[must_use]
pub fn base64url(bytes: &[u8]) -> String {
    encode(
        bytes,
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_",
        false,
    )
}

fn encode(bytes: &[u8], alphabet: &[u8; 64], pad: bool) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0_u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        // A chunk of 1, 2 or 3 bytes gives 2, 3 or 4 characters.
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(alphabet[((n >> (18 - 6 * i)) & 63) as usize]));
            } else if pad {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_4648_with_and_without_padding() {
        for (input, padded, url) in [
            (&b""[..], "", ""),
            (b"f", "Zg==", "Zg"),
            (b"fo", "Zm8=", "Zm8"),
            (b"foo", "Zm9v", "Zm9v"),
            (b"foob", "Zm9vYg==", "Zm9vYg"),
            (b"fooba", "Zm9vYmE=", "Zm9vYmE"),
            (b"foobar", "Zm9vYmFy", "Zm9vYmFy"),
            (&[0xfb, 0xff, 0xfe], "+//+", "-__-"),
        ] {
            assert_eq!(base64(input), padded, "{input:?}");
            assert_eq!(base64url(input), url, "{input:?}");
        }
    }

    #[test]
    fn query_values_are_percent_encoded() {
        assert_eq!(
            percent_encode("http://localhost:1/callback"),
            "http%3A%2F%2Flocalhost%3A1%2Fcallback"
        );
        assert_eq!(
            percent_encode("Vocet on this Mac ~é"),
            "Vocet%20on%20this%20Mac%20~%C3%A9"
        );
    }
}
