//! How long a transcription request may take, and how often a timeout is
//! retried, as a function of the clip's length (H1).
//!
//! A fixed 30 s limit fails a long dictation on a slow connection: five
//! minutes of 16 kHz mono WAV is about 9.6 MB, which needs about 2.6 Mbps
//! upstream to finish in 30 s. So the limit grows with the audio, and a
//! long clip that does time out isn't retried over and over.
//!
//! Pure: durations in, durations out.

use std::time::Duration;

/// The extra time a request gets for each second of audio. A 5-minute clip
/// (about 9.6 MB) gets 30 s + 150 s = 180 s, enough for about 0.45 Mbps
/// upstream instead of 2.6 Mbps.
pub const TIMEOUT_PER_AUDIO_SECOND: Duration = Duration::from_millis(500);

/// A clip this long or longer is retried after a timeout at most
/// [`LONG_CLIP_MAX_TIMEOUT_RETRIES`] times.
pub const LONG_CLIP: Duration = Duration::from_secs(60);

/// How many times a timed-out request for a [`LONG_CLIP`] is tried again.
/// Each attempt can take minutes, so three would leave the user waiting
/// close to ten.
pub const LONG_CLIP_MAX_TIMEOUT_RETRIES: u32 = 1;

/// The limit for one request: `base` plus [`TIMEOUT_PER_AUDIO_SECOND`] for
/// each second of `clip`. Saturates instead of overflowing.
#[must_use]
pub fn request_timeout(base: Duration, clip: Duration) -> Duration {
    // `clip / 2` is `TIMEOUT_PER_AUDIO_SECOND` per second of audio (a test
    // pins the two together).
    base.saturating_add(clip / 2)
}

/// Whether a request that has now timed out `timeouts` times (the one that
/// just failed included, from 1) may be tried again. Clips shorter than
/// `long_clip` ([`LONG_CLIP`] in the app) are not limited here (the retry
/// policy's attempts still apply); a longer one gets
/// [`LONG_CLIP_MAX_TIMEOUT_RETRIES`] retries.
#[must_use]
pub fn may_retry_after_timeout(clip: Duration, long_clip: Duration, timeouts: u32) -> bool {
    clip < long_clip || timeouts <= LONG_CLIP_MAX_TIMEOUT_RETRIES
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: Duration = Duration::from_secs(30);

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn the_limit_grows_half_a_second_per_second_of_audio() {
        assert_eq!(request_timeout(BASE, secs(300)), secs(180));
        assert_eq!(request_timeout(BASE, secs(60)), secs(60));
        assert_eq!(request_timeout(BASE, Duration::ZERO), BASE);
    }

    #[test]
    fn short_clips_stay_close_to_today() {
        assert_eq!(request_timeout(BASE, secs(10)), secs(35));
        assert_eq!(
            request_timeout(BASE, Duration::from_millis(250)),
            Duration::from_millis(30_125)
        );
    }

    #[test]
    fn the_limit_never_overflows() {
        assert_eq!(request_timeout(Duration::MAX, secs(1)), Duration::MAX);
        assert_eq!(
            request_timeout(BASE, Duration::MAX),
            Duration::MAX / 2 + BASE
        );
    }

    #[test]
    fn the_constant_matches_the_arithmetic() {
        assert_eq!(TIMEOUT_PER_AUDIO_SECOND * 2, secs(1));
    }

    #[test]
    fn short_clips_are_not_limited_here() {
        for timeouts in [1, 2, 3, 100] {
            assert!(may_retry_after_timeout(
                Duration::from_millis(59_999),
                LONG_CLIP,
                timeouts
            ));
        }
    }

    #[test]
    fn a_long_clip_is_retried_once_after_a_timeout() {
        assert!(may_retry_after_timeout(LONG_CLIP, LONG_CLIP, 1));
        assert!(!may_retry_after_timeout(LONG_CLIP, LONG_CLIP, 2));
        assert!(!may_retry_after_timeout(secs(300), LONG_CLIP, 3));
    }
}
