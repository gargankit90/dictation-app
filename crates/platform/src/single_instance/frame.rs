//! The wire format between a second launch and the running instance.
//!
//! A frame is a 4-byte big-endian body length, then the body: a 4-byte
//! big-endian string count, then each string as a 4-byte big-endian byte
//! length and its UTF-8 bytes. The first string is the launch's working
//! directory, the rest are its arguments. The running instance answers with
//! one byte, [`ACK`] once the launch's message has been handled or [`NAK`]
//! if the frame was invalid.
//!
//! Lengths are checked against [`MAX_FRAME`] before anything is allocated,
//! so a peer can't make the running instance buffer more than that.

use std::io;

use super::Message;

/// The largest body either side accepts: far more than any real command
/// line, small enough that a hostile local peer can't exhaust memory.
pub const MAX_FRAME: usize = 64 * 1024;

/// "Message handled."
pub const ACK: u8 = 0x06;
/// "Invalid frame; not handled."
pub const NAK: u8 = 0x15;

/// The byte length of the header in front of the body.
pub const HEADER: usize = 4;

/// The frame for `message`: header and body.
///
/// # Errors
///
/// [`io::ErrorKind::InvalidInput`] if the body would exceed [`MAX_FRAME`].
pub fn encode(message: &Message) -> io::Result<Vec<u8>> {
    let strings = std::iter::once(&message.cwd).chain(&message.args);
    let mut body = Vec::new();
    push_len(&mut body, 1 + message.args.len())?;
    for string in strings {
        push_len(&mut body, string.len())?;
        body.extend_from_slice(string.as_bytes());
        if body.len() > MAX_FRAME {
            return Err(too_long());
        }
    }
    let mut frame = Vec::with_capacity(HEADER + body.len());
    push_len(&mut frame, body.len())?;
    frame.extend_from_slice(&body);
    Ok(frame)
}

/// The body length a header announces.
///
/// # Errors
///
/// [`io::ErrorKind::InvalidData`] if it exceeds [`MAX_FRAME`].
pub fn body_len(header: [u8; HEADER]) -> io::Result<usize> {
    let len = usize::try_from(u32::from_be_bytes(header)).map_err(|_| invalid("length"))?;
    if len > MAX_FRAME {
        return Err(invalid("frame longer than the limit"));
    }
    Ok(len)
}

/// The message in a frame's body.
///
/// # Errors
///
/// [`io::ErrorKind::InvalidData`] if the body is truncated, has bytes left
/// over, has no working directory or holds a string that isn't UTF-8.
pub fn decode(body: &[u8]) -> io::Result<Message> {
    let mut rest = body;
    let count = take_len(&mut rest)?;
    if count == 0 {
        return Err(invalid("no working directory"));
    }
    // Each string needs at least its 4-byte length, so a huge count in a
    // short body fails here instead of reserving memory for it.
    if count > rest.len() / HEADER {
        return Err(invalid("more strings than the body holds"));
    }
    let mut strings = Vec::with_capacity(count);
    for _ in 0..count {
        let len = take_len(&mut rest)?;
        let (bytes, tail) = rest
            .split_at_checked(len)
            .ok_or_else(|| invalid("truncated string"))?;
        let string = std::str::from_utf8(bytes).map_err(|_| invalid("string isn't UTF-8"))?;
        strings.push(string.to_owned());
        rest = tail;
    }
    if !rest.is_empty() {
        return Err(invalid("bytes after the last string"));
    }
    let mut strings = strings.into_iter();
    let cwd = strings.next().unwrap_or_default();
    Ok(Message {
        cwd,
        args: strings.collect(),
    })
}

fn push_len(buf: &mut Vec<u8>, len: usize) -> io::Result<()> {
    let len = u32::try_from(len).map_err(|_| too_long())?;
    buf.extend_from_slice(&len.to_be_bytes());
    Ok(())
}

fn take_len(rest: &mut &[u8]) -> io::Result<usize> {
    let (len, tail) = rest
        .split_first_chunk::<HEADER>()
        .ok_or_else(|| invalid("truncated length"))?;
    *rest = tail;
    usize::try_from(u32::from_be_bytes(*len)).map_err(|_| invalid("length"))
}

fn too_long() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "the arguments are too long to forward to the running instance",
    )
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("invalid single-instance message: {what}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(cwd: &str, args: &[&str]) -> Message {
        Message {
            cwd: cwd.to_owned(),
            args: args.iter().map(|&arg| arg.to_owned()).collect(),
        }
    }

    fn body(frame: &[u8]) -> &[u8] {
        let (header, body) = frame.split_first_chunk::<HEADER>().unwrap();
        assert_eq!(body_len(*header).unwrap(), body.len());
        body
    }

    #[test]
    fn round_trips() {
        for original in [
            message("/tmp", &["/app", "--release"]),
            message("", &[]),
            message("/ü", &["", "a\0b", "--press"]),
        ] {
            let frame = encode(&original).unwrap();
            assert_eq!(decode(body(&frame)).unwrap(), original);
        }
    }

    #[test]
    fn refuses_to_encode_more_than_the_limit() {
        let long = "x".repeat(MAX_FRAME);
        let error = encode(&message("/", &[&long])).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn encodes_up_to_the_limit() {
        // count + cwd length + arg length = 12 bytes of overhead.
        let arg = "x".repeat(MAX_FRAME - 12);
        let frame = encode(&message("", &[&arg])).unwrap();
        assert_eq!(frame.len(), HEADER + MAX_FRAME);
    }

    #[test]
    fn rejects_a_header_over_the_limit() {
        let too_long = u32::try_from(MAX_FRAME + 1).unwrap().to_be_bytes();
        assert_eq!(
            body_len(too_long).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert!(body_len(u32::MAX.to_be_bytes()).is_err());
    }

    #[test]
    fn rejects_garbled_bodies() {
        let good = encode(&message("/tmp", &["--release"])).unwrap();
        let good = body(&good);
        let mut extra = good.to_vec();
        extra.push(0);
        let mut bad_utf8 = good.to_vec();
        *bad_utf8.last_mut().unwrap() = 0xff;
        let huge_count = [u32::MAX.to_be_bytes(), 0u32.to_be_bytes()].concat();
        let long_string = [1u32.to_be_bytes(), 100u32.to_be_bytes()].concat();

        for garbled in [
            &[][..],
            &[0, 0][..],
            &0u32.to_be_bytes()[..],
            &good[..good.len() - 1],
            &extra[..],
            &bad_utf8[..],
            &huge_count[..],
            &long_string[..],
        ] {
            let error = decode(garbled).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData, "{garbled:?}");
        }
    }
}
