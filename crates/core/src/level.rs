//! The live input level shown in the recording pill (U4): RMS to dBFS, and
//! when a recording has been quiet long enough to say "No sound".
//!
//! Pure: the caller passes the time, so tests drive a fake clock.

use std::time::Duration;

/// The lowest level reported, in dBFS: digital silence (an RMS of zero)
/// reads as this, not as minus infinity. Below the noise floor of 16-bit
/// audio (about −96 dBFS) any real microphone has.
pub const LEVEL_FLOOR_DBFS: f32 = -100.0;

/// Below this level the pill counts the input as silent, in dBFS (RMS):
/// −50 dBFS, the transcription gate's
/// [`DEFAULT_SILENCE_THRESHOLD`](crate::audio_math::DEFAULT_SILENCE_THRESHOLD).
///
/// Chosen to match that gate on purpose: a recording the pill calls "No
/// sound" is one the pipeline would also drop as silence, so the pill never
/// says "No sound" about audio that would be transcribed, and never shows
/// Listening calmly over audio that will be thrown away. Laptop and headset
/// microphones in a quiet room sit around −60 to −50 dBFS and speech on the
/// built-in microphone around −40 (issue #72), so a working microphone
/// crosses it as soon as the user speaks; a muted, unplugged or wrong
/// input stays below it.
pub const NO_SOUND_THRESHOLD_DBFS: f32 = -50.0;

/// How long a recording must stay below [`NO_SOUND_THRESHOLD_DBFS`], from
/// its start, before the pill says "No sound" (U13). Only while nothing has
/// been heard yet: people pause mid-sentence for longer than this.
pub const NO_SOUND_AFTER: Duration = Duration::from_millis(2_500);

/// An RMS (`0.0..=1.0`) in dBFS: `20 · log10(rms)`, never below
/// [`LEVEL_FLOOR_DBFS`]. A sine whose RMS is 0.1 reads −20 dBFS (its peak is
/// 0.141). Zero, negative and non-finite input read as the floor; values
/// above 1.0 as 0 dBFS.
#[must_use]
pub fn rms_to_dbfs(rms: f32) -> f32 {
    if !rms.is_finite() || rms <= 0.0 {
        return LEVEL_FLOOR_DBFS;
    }
    (20.0 * rms.min(1.0).log10()).max(LEVEL_FLOOR_DBFS)
}

/// Whether a level counts as silent for "No sound".
#[must_use]
pub fn is_quiet(dbfs: f32) -> bool {
    // NaN is not quiet: a broken reading never claims the mic is dead.
    dbfs < NO_SOUND_THRESHOLD_DBFS
}

/// Watches one recording's levels for "No sound": silent only while nothing
/// has been heard yet in this hold and every level for [`NO_SOUND_AFTER`] has
/// been quiet. It goes back to loud at the first level that isn't quiet, and
/// once the input has been loud it never reports silent again (U13): a pause
/// after speech is a thinking pause, not a dead microphone. Times are any
/// monotonic clock's (a [`Duration`] since some start), passed in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SilenceWatch {
    /// When the current quiet run began; `None` before the first level.
    quiet_since: Option<Duration>,
    /// Whether any level was loud in this hold.
    heard: bool,
}

impl SilenceWatch {
    /// A watch for a recording that starts now: not silent yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes a level read at `now`; returns whether the input counts as
    /// silent ("No sound") from now on.
    pub fn on_level(&mut self, dbfs: f32, now: Duration) -> bool {
        if !is_quiet(dbfs) {
            self.heard = true;
            self.quiet_since = None;
            return false;
        }
        if self.heard {
            return false;
        }
        let since = *self.quiet_since.get_or_insert(now);
        now.saturating_sub(since) >= NO_SOUND_AFTER
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_math::DEFAULT_SILENCE_THRESHOLD;

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn rms_maps_to_dbfs() {
        assert!((rms_to_dbfs(1.0) - 0.0).abs() < 1e-4);
        assert!((rms_to_dbfs(0.1) + 20.0).abs() < 1e-4);
        assert!((rms_to_dbfs(0.001) + 60.0).abs() < 1e-3);
        assert!((rms_to_dbfs(2.0) - 0.0).abs() < 1e-4);
    }

    #[test]
    fn silence_and_junk_read_as_the_floor() {
        for rms in [0.0, -0.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1e-12] {
            assert_eq!(rms_to_dbfs(rms), LEVEL_FLOOR_DBFS, "{rms}");
        }
    }

    #[test]
    fn the_threshold_is_the_transcription_gates() {
        let gate = rms_to_dbfs(DEFAULT_SILENCE_THRESHOLD);
        assert!((gate - NO_SOUND_THRESHOLD_DBFS).abs() < 0.01, "{gate}");
        assert!(is_quiet(-50.1));
        assert!(!is_quiet(-50.0));
        assert!(!is_quiet(f32::NAN));
    }

    #[test]
    fn criterion_u13_1_silence_from_the_start_says_no_sound_at_2_5_s_not_before() {
        let mut watch = SilenceWatch::new();
        assert!(!watch.on_level(LEVEL_FLOOR_DBFS, ms(0)));
        assert!(!watch.on_level(-70.0, ms(1_500)));
        assert!(!watch.on_level(-70.0, ms(2_499)));
        assert!(watch.on_level(-70.0, ms(2_500)));
        assert!(watch.on_level(-70.0, ms(4_000)));
    }

    #[test]
    fn criterion_u13_1_speech_then_a_long_pause_is_never_no_sound() {
        let mut watch = SilenceWatch::new();
        assert!(!watch.on_level(-30.0, ms(0)));
        for step in 1..=100_u64 {
            assert!(!watch.on_level(-80.0, ms(step * 50)), "{step}");
        }
        assert!(!watch.on_level(-80.0, ms(60_000)));
    }

    #[test]
    fn criterion_u13_1_silence_then_speech_goes_back_to_listening_for_good() {
        let mut watch = SilenceWatch::new();
        watch.on_level(-80.0, ms(0));
        assert!(watch.on_level(-80.0, ms(3_000)));
        assert!(!watch.on_level(-30.0, ms(3_050)));
        // Quiet again after speech: never No sound.
        assert!(!watch.on_level(-80.0, ms(3_100)));
        assert!(!watch.on_level(-80.0, ms(9_000)));
    }

    #[test]
    fn a_short_pause_between_words_is_not_silence() {
        let mut watch = SilenceWatch::new();
        for step in 0..60_u64 {
            let now = ms(step * 50);
            let level = if step % 20 == 0 { -30.0 } else { -70.0 };
            assert!(!watch.on_level(level, now), "{now:?}");
        }
    }
}
