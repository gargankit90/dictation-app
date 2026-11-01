# coreaudio-rs 0.14.2, patched

Upstream: <https://github.com/RustAudio/coreaudio-rs>, the crates.io release
0.14.2 (git `f07c02c7419328650112d4b276cffd785b0be4ee`), MIT OR Apache-2.0
(`LICENSE-MIT`, `LICENSE-APACHE`). It reaches dictation-app through
handy-recorder 0.1.0 → handy-cpal 0.18.2, and the root `Cargo.toml` swaps it
in with `[patch.crates-io]`.

The unmodified release is its own commit ("chore(deps): vendor coreaudio-rs
0.14.2 unmodified"), so `git diff <that commit> -- third_party/coreaudio-rs`
shows exactly this patch. Registry metadata (`.cargo_vcs_info.json`,
`.cargo-ok`), `.github/` and `Cargo.lock` were left out. `rustfmt.toml`
(formatting off) is ours: it keeps the code as published under our edition
2024 pre-commit check. Code marked `Patched (PATCHES.md)` is ours.

Drop this directory and the `[patch.crates-io]` entry once a coreaudio-rs
release fixes the bug and handy-cpal depends on it.

## The bug (dictation-app #77, B2)

`AudioUnit::set_input_callback` allocates the `AudioBufferList` data the input
callback renders into as a `Vec<u8>`, forgets it, and keeps only the pointer.

- When the callback's frame count changes, the callback (on the real-time audio
  thread) calls `AudioUnitGetProperty` for the stream format, rebuilds the
  `Vec` with `Vec::from_raw_parts` and `resize`s it. Growth reallocates on the
  audio thread.
- On shrink, `resize` shortens the length but keeps the capacity, which is
  remembered only inside the callback closure. `free_input_callback` (called
  from `AudioUnit::drop`) rebuilds the `Vec` with `cap = mDataByteSize`, the
  current length, not the real capacity: `Vec::from_raw_parts` with the wrong
  capacity, so the deallocation layout doesn't match the allocation. That is
  undefined behaviour. Growth can do the same, since `resize` may round the
  capacity up.

## The patch

In `src/audio_unit/render_callback.rs` and `src/audio_unit/mod.rs`:

1. **`InputBuffer`** owns the one-buffer `AudioBufferList` and its data. It is
   allocated once, in `set_input_callback` (off the audio thread), and records
   the real capacity. Its `Drop` frees the data with that capacity. It lives in
   `InputCallback` (replacing the raw `buffer_list` pointer), so
   `free_input_callback` just drops it.
2. **Room for the largest callback.** The capacity is the largest of the
   current I/O buffer size, the unit's `kAudioUnitProperty_MaximumFramesPerSlice`,
   the device's `kAudioDevicePropertyBufferFrameSizeRange` maximum (macOS) and
   4096 frames (`MIN_INPUT_BUFFER_FRAMES`). That's 32 KiB for stereo f32.
3. **The callback never allocates or queries.** Each call sets the buffer's
   pointer and `mDataByteSize` for its frame count (`InputBufferRef::set_len`).
   It no longer reads the stream format: the client format on the input element
   is fixed while the callback is set (the unit converts the device's audio to
   it), and the callback was built for it.
4. **A bounded, non-blocking report for oversized blocks.** A callback larger
   than the buffer increments an `AtomicU64` and returns
   `kAudioUnitErr_TooManyFramesToProcess` without rendering.
   `AudioUnit::input_callback_oversized_blocks()` reads the count. That's the
   only API added.

Size: about 170 lines added and 80 removed outside the tests, plus a
`patch_tests` module (about 170 lines).

## Tests

```sh
CARGO_TARGET_DIR=target/third_party \
  cargo test --manifest-path third_party/coreaudio-rs/Cargo.toml --lib patch_tests
# Pure-Rust part under Miri (needs `rustup component add --toolchain nightly miri rust-src`):
cd third_party/coreaudio-rs && CARGO_TARGET_DIR=../../target/third_party-miri \
  cargo +nightly miri test --lib patch_tests
```

A test global allocator counts allocations and records the size the buffer is
freed with.

- `the_buffer_grows_and_shrinks_in_place_and_frees_its_capacity`: `set_len`
  across growth, shrink and an oversized length makes no allocations, and drop
  frees the full capacity. It also runs under Miri, which checks the
  deallocation layout itself.
- `the_input_callback_survives_growth_shrink_and_teardown` (macOS): a real HAL
  input unit on the default input, with the real callback closure called
  directly for 512, 1024, 256, max, 1, 4096 and 128 frames, then max + 1. The
  unit is never initialized or started, so nothing records and no permission
  is needed; `AudioUnitRender` returns `kAudioUnitErr_Uninitialized` after the
  callback has sized the buffer. Checks: no allocations, the oversized block
  is counted, and `drop(unit)` frees the buffer with its real capacity. With
  upstream's teardown put back, it fails (`left: 1024, right: 32768`).

The workspace excludes `third_party/`, so the gate (CI on Linux) doesn't run
these. They run on a Mac with the commands above.
