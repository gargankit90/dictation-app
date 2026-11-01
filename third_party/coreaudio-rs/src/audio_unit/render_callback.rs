use objc2_audio_toolbox::{
    kAudioOutputUnitProperty_SetInputCallback, kAudioUnitErr_TooManyFramesToProcess,
    kAudioUnitProperty_MaximumFramesPerSlice, kAudioUnitProperty_SetRenderCallback,
    AURenderCallbackStruct, AudioUnitRender, AudioUnitRenderActionFlags,
};
use objc2_core_audio_types::{AudioBuffer, AudioBufferList, AudioTimeStamp};

use super::audio_format::LinearPcmFlags;
use super::{AudioUnit, Element, Scope};
use crate::error::{self, Error};
use crate::OSStatus;
use std::convert::TryFrom;
use std::mem;
use std::os::raw::c_void;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub use self::action_flags::ActionFlags;
pub use self::data::Data;

/// When `set_render_callback` is called, a closure of this type will be used to wrap the given
/// render callback function.
///
/// This allows the user to provide a custom, more rust-esque callback function type that takes
/// greater advantage of rust's type safety.
pub type InputProcFn = dyn FnMut(
    NonNull<AudioUnitRenderActionFlags>,
    NonNull<AudioTimeStamp>,
    u32,
    u32,
    *mut AudioBufferList,
) -> OSStatus;

/// This type allows us to safely wrap a boxed `RenderCallback` to use within the input proc.
pub struct InputProcFnWrapper {
    callback: Box<InputProcFn>,
}

/// Patched (PATCHES.md): the fewest frames an input buffer holds, in case the
/// unit and the device can't say how many a callback may ask for.
const MIN_INPUT_BUFFER_FRAMES: u32 = 4096;

/// Patched (PATCHES.md): the one-buffer `AudioBufferList` an input callback
/// renders into, and the allocation behind it.
///
/// It is allocated once, off the audio thread, with room for the largest
/// callback. The callback only changes the length the buffer reports
/// ([`InputBufferRef::set_len`]), so the audio thread never allocates, and
/// `Drop` frees the allocation with the capacity it was made with.
pub(crate) struct InputBuffer {
    buffer: InputBufferRef,
}

/// What the input callback holds of its [`InputBuffer`].
#[derive(Clone, Copy)]
pub(crate) struct InputBufferRef {
    list: *mut AudioBufferList,
    data: *mut u8,
    capacity: usize,
}

impl InputBuffer {
    /// `capacity` zeroed bytes of `n_channels` interleaved audio.
    pub(crate) fn new(capacity: usize, n_channels: u32) -> Self {
        let mut data = mem::ManuallyDrop::new(vec![0u8; capacity]);
        let capacity = data.capacity();
        let data = data.as_mut_ptr();
        let list = Box::into_raw(Box::new(AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: n_channels,
                mDataByteSize: u32::try_from(capacity).unwrap_or(u32::MAX),
                mData: data as *mut _,
            }],
        }));
        InputBuffer {
            buffer: InputBufferRef {
                list,
                data,
                capacity,
            },
        }
    }

    /// For the callback; valid while `self` lives.
    pub(crate) fn as_ref(&self) -> InputBufferRef {
        self.buffer
    }
}

impl Drop for InputBuffer {
    fn drop(&mut self) {
        let InputBufferRef {
            list,
            data,
            capacity,
        } = self.buffer;
        // SAFETY: `list` came from `Box::into_raw` and `data` from a
        // `Vec<u8>` of exactly `capacity`, both in `new`, and are freed only
        // here. Length 0 is within the capacity, and `u8` needs no drop.
        unsafe {
            drop(Box::from_raw(list));
            drop(Vec::from_raw_parts(data, 0, capacity));
        }
    }
}

impl InputBufferRef {
    /// The list to render into.
    pub(crate) fn list(self) -> *mut AudioBufferList {
        self.list
    }

    /// Makes the buffer report `bytes` bytes at the start of the allocation
    /// (`AudioUnitRender` may change its pointer and size). Returns false,
    /// changing nothing, if `bytes` doesn't fit. Never allocates.
    ///
    /// # Safety
    ///
    /// The [`InputBuffer`] this came from must be alive, and nothing else may
    /// use its list during the call.
    pub(crate) unsafe fn set_len(self, bytes: usize) -> bool {
        let size = match u32::try_from(bytes) {
            Ok(size) if bytes <= self.capacity => size,
            _ => return false,
        };
        // SAFETY: the caller keeps the list alive and unshared; it holds one
        // buffer (`mNumberBuffers: 1`).
        let buffer = unsafe { &mut (*self.list).mBuffers[0] };
        buffer.mData = self.data as *mut _;
        buffer.mDataByteSize = size;
        true
    }
}

/// Patched (PATCHES.md): the most frames an input callback may ask for: the
/// largest of the current I/O buffer size, the unit's maximum frames per
/// slice, the device's largest I/O buffer size (macOS) and
/// [`MIN_INPUT_BUFFER_FRAMES`].
fn max_input_frames(unit: &AudioUnit, current: u32) -> u32 {
    let per_slice: u32 = unit
        .get_property(
            kAudioUnitProperty_MaximumFramesPerSlice,
            Scope::Global,
            Element::Output,
        )
        .unwrap_or(0);
    #[cfg(target_os = "macos")]
    let device_max: u32 = unit
        .get_property::<objc2_core_audio_types::AudioValueRange>(
            objc2_core_audio::kAudioDevicePropertyBufferFrameSizeRange,
            Scope::Global,
            Element::Output,
        )
        .map_or(0, |range| range.mMaximum as u32);
    #[cfg(not(target_os = "macos"))]
    let device_max: u32 = 0;
    current
        .max(per_slice)
        .max(device_max)
        .max(MIN_INPUT_BUFFER_FRAMES)
}

/// Arguments given to the render callback function.
#[derive(Debug)]
pub struct Args<D> {
    /// A type wrapping the the buffer that matches the expected audio format.
    pub data: D,
    /// Timing information for the callback.
    pub time_stamp: AudioTimeStamp,
    /// TODO
    pub bus_number: u32,
    /// The number of frames in the buffer as `usize` for easier indexing.
    pub num_frames: usize,
    /// Flags for configuring audio unit rendering.
    ///
    /// This parameter lets a callback provide various hints to the audio unit.
    ///
    /// For example: if there is no audio to process, we can insert the `OUTPUT_IS_SILENCE` flag to
    /// indicate to the audio unit that the buffer does not need to be processed.
    pub flags: action_flags::Handle,
}

/// Format specific render callback data.
pub mod data {
    use objc2_core_audio_types::AudioBuffer;
    use objc2_core_audio_types::AudioBufferList;

    use super::super::Sample;
    use super::super::StreamFormat;
    use crate::audio_unit::audio_format::LinearPcmFlags;
    use std::marker::PhantomData;
    use std::slice;

    /// Audio data wrappers specific to the `AudioUnit`'s `AudioFormat`.
    pub trait Data {
        /// Check whether the stream format matches this type of data.
        fn does_stream_format_match(stream_format: &StreamFormat) -> bool;
        /// We must be able to construct Self from arguments given to the `input_proc`.
        /// # Safety
        /// TODO document how to use this function safely.
        unsafe fn from_input_proc_args(num_frames: u32, io_data: *mut AudioBufferList) -> Self;
    }

    /// A raw pointer to the audio data so that the user may handle it themselves.
    #[derive(Debug)]
    pub struct Raw {
        pub data: *mut AudioBufferList,
    }

    impl Data for Raw {
        fn does_stream_format_match(_: &StreamFormat) -> bool {
            true
        }
        unsafe fn from_input_proc_args(_num_frames: u32, io_data: *mut AudioBufferList) -> Self {
            Raw { data: io_data }
        }
    }

    /// An interleaved linear PCM buffer with samples of type `S`.
    pub struct Interleaved<S: 'static> {
        /// The audio buffer.
        pub buffer: &'static mut [S],
        pub channels: usize,
        sample_format: PhantomData<S>,
    }

    /// An interleaved linear PCM buffer with samples stored as plain bytes.
    pub struct InterleavedBytes<S: 'static> {
        /// The audio buffer.
        pub buffer: &'static mut [u8],
        pub channels: usize,
        sample_format: PhantomData<S>,
    }

    /// A wrapper around the pointer to the `mBuffers` array.
    pub struct NonInterleaved<S> {
        /// The list of audio buffers.
        buffers: &'static mut [AudioBuffer],
        /// The number of frames in each channel.
        frames: usize,
        sample_format: PhantomData<S>,
    }

    /// An iterator produced by a `NonInterleaved`, yielding a reference to each channel.
    pub struct Channels<'a, S: 'a> {
        buffers: slice::Iter<'a, AudioBuffer>,
        frames: usize,
        sample_format: PhantomData<S>,
    }

    /// An iterator produced by a `NonInterleaved`, yielding a mutable reference to each channel.
    pub struct ChannelsMut<'a, S: 'a> {
        buffers: slice::IterMut<'a, AudioBuffer>,
        frames: usize,
        sample_format: PhantomData<S>,
    }

    unsafe impl<S> Send for NonInterleaved<S> where S: Send {}

    impl<'a, S> Iterator for Channels<'a, S> {
        type Item = &'a [S];
        #[allow(non_snake_case)]
        fn next(&mut self) -> Option<Self::Item> {
            self.buffers.next().map(
                |&AudioBuffer {
                     mNumberChannels,
                     mData,
                     ..
                 }| {
                    let len = mNumberChannels as usize * self.frames;
                    let ptr = mData as *mut S;
                    unsafe { slice::from_raw_parts(ptr, len) }
                },
            )
        }
    }

    impl<'a, S> Iterator for ChannelsMut<'a, S> {
        type Item = &'a mut [S];
        #[allow(non_snake_case)]
        fn next(&mut self) -> Option<Self::Item> {
            self.buffers.next().map(
                |&mut AudioBuffer {
                     mNumberChannels,
                     mData,
                     ..
                 }| {
                    let len = mNumberChannels as usize * self.frames;
                    let ptr = mData as *mut S;
                    unsafe { slice::from_raw_parts_mut(ptr, len) }
                },
            )
        }
    }

    impl<S> NonInterleaved<S> {
        /// An iterator yielding a reference to each channel in the array.
        pub fn channels(&self) -> Channels<'_, S> {
            Channels {
                buffers: self.buffers.iter(),
                frames: self.frames,
                sample_format: PhantomData,
            }
        }

        /// An iterator yielding a mutable reference to each channel in the array.
        pub fn channels_mut(&mut self) -> ChannelsMut<'_, S> {
            ChannelsMut {
                buffers: self.buffers.iter_mut(),
                frames: self.frames,
                sample_format: PhantomData,
            }
        }
    }

    // Implementation for a non-interleaved linear PCM audio format.
    impl<S> Data for NonInterleaved<S>
    where
        S: Sample,
    {
        fn does_stream_format_match(stream_format: &StreamFormat) -> bool {
            stream_format
                .flags
                .contains(LinearPcmFlags::IS_NON_INTERLEAVED)
                && S::sample_format().does_match_flags(stream_format.flags)
        }

        #[allow(non_snake_case)]
        unsafe fn from_input_proc_args(frames: u32, io_data: *mut AudioBufferList) -> Self {
            let ptr = (*io_data).mBuffers.as_ptr() as *mut AudioBuffer;
            let len = (*io_data).mNumberBuffers as usize;
            let buffers = slice::from_raw_parts_mut(ptr, len);
            NonInterleaved {
                buffers,
                frames: frames as usize,
                sample_format: PhantomData,
            }
        }
    }

    // Implementation for an interleaved linear PCM audio format.
    impl<S> Data for Interleaved<S>
    where
        S: Sample,
    {
        fn does_stream_format_match(stream_format: &StreamFormat) -> bool {
            !stream_format
                .flags
                .contains(LinearPcmFlags::IS_NON_INTERLEAVED)
                && S::sample_format().does_match_flags(stream_format.flags)
        }

        #[allow(non_snake_case)]
        unsafe fn from_input_proc_args(frames: u32, io_data: *mut AudioBufferList) -> Self {
            // // We're expecting a single interleaved buffer which will be the first in the array.
            let AudioBuffer {
                mNumberChannels,
                mDataByteSize,
                mData,
            } = (*io_data).mBuffers[0];
            // // Ensure that the size of the data matches the size of the sample format
            // // multiplied by the number of frames.
            // //
            // // TODO: Return an Err instead of `panic`ing.
            let buffer_len = frames as usize * mNumberChannels as usize;
            let expected_size = ::std::mem::size_of::<S>() * buffer_len;
            assert!(mDataByteSize as usize == expected_size);

            let buffer: &mut [S] = {
                let buffer_ptr = mData as *mut S;
                slice::from_raw_parts_mut(buffer_ptr, buffer_len)
            };

            Interleaved {
                buffer,
                channels: mNumberChannels as usize,
                sample_format: PhantomData,
            }
        }
    }

    // Implementation for an interleaved linear PCM audio format using plain bytes.
    impl<S> Data for InterleavedBytes<S>
    where
        S: Sample,
    {
        fn does_stream_format_match(stream_format: &StreamFormat) -> bool {
            !stream_format
                .flags
                .contains(LinearPcmFlags::IS_NON_INTERLEAVED)
                && S::sample_format().does_match_flags(stream_format.flags)
        }

        #[allow(non_snake_case)]
        unsafe fn from_input_proc_args(frames: u32, io_data: *mut AudioBufferList) -> Self {
            // // We're expecting a single interleaved buffer which will be the first in the array.
            let AudioBuffer {
                mNumberChannels,
                mDataByteSize,
                mData,
            } = (*io_data).mBuffers[0];
            // // Ensure that the size of the data matches the size of the sample format
            // // multiplied by the number of frames.
            // //
            // // TODO: Return an Err instead of `panic`ing.
            let buffer_len = frames as usize * mNumberChannels as usize;
            let expected_size = ::std::mem::size_of::<S>() * buffer_len;
            assert!(mDataByteSize as usize == expected_size);

            let buffer: &mut [u8] = {
                let buffer_ptr = mData as *mut u8;
                slice::from_raw_parts_mut(buffer_ptr, mDataByteSize as usize)
            };

            InterleavedBytes {
                buffer,
                channels: mNumberChannels as usize,
                sample_format: PhantomData,
            }
        }
    }
}

pub mod action_flags {
    use objc2_audio_toolbox::AudioUnitRenderActionFlags;

    use std::fmt;

    bitflags! {
        pub struct ActionFlags: u32 {
            /// Called on a render notification Proc, which is called either before or after the
            /// render operation of the audio unit. If this flag is set, the proc is being called
            /// before the render operation is performed.
            ///
            /// **Available** in OS X v10.0 and later.
            const PRE_RENDER = AudioUnitRenderActionFlags::UnitRenderAction_PreRender.0;
            /// Called on a render notification Proc, which is called either before or after the
            /// render operation of the audio unit. If this flag is set, the proc is being called
            /// after the render operation is completed.
            ///
            /// **Available** in OS X v10.0 and later.
            const POST_RENDER = AudioUnitRenderActionFlags::UnitRenderAction_PostRender.0;
            /// This flag can be set in a render input callback (or in the audio unit's render
            /// operation itself) and is used to indicate that the render buffer contains only
            /// silence. It can then be used by the caller as a hint to whether the buffer needs to
            /// be processed or not.
            ///
            /// **Available** in OS X v10.2 and later.
            const OUTPUT_IS_SILENCE = AudioUnitRenderActionFlags::UnitRenderAction_OutputIsSilence.0;
            /// This is used with offline audio units (of type 'auol'). It is used when an offline
            /// unit is being preflighted, which is performed prior to when the actual offline
            /// rendering actions are performed. It is used for those cases where the offline
            /// process needs it (for example, with an offline unit that normalizes an audio file,
            /// it needs to see all of the audio data first before it can perform its
            /// normalization).
            ///
            /// **Available** in OS X v10.3 and later.
            const OFFLINE_PREFLIGHT = AudioUnitRenderActionFlags::OfflineUnitRenderAction_Preflight.0;
            /// Once an offline unit has been successfully preflighted, it is then put into its
            /// render mode. This flag is set to indicate to the audio unit that it is now in that
            /// state and that it should perform processing on the input data.
            ///
            /// **Available** in OS X v10.3 and later.
            const OFFLINE_RENDER = AudioUnitRenderActionFlags::OfflineUnitRenderAction_Render.0;
            /// This flag is set when an offline unit has completed either its preflight or
            /// performed render operation.
            ///
            /// **Available** in OS X v10.3 and later.
            const OFFLINE_COMPLETE = AudioUnitRenderActionFlags::OfflineUnitRenderAction_Complete.0;
            /// If this flag is set on the post-render call an error was returned by the audio
            /// unit's render operation. In this case, the error can be retrieved through the
            /// `lastRenderError` property and the audio data in `ioData` handed to the post-render
            /// notification will be invalid.
            ///
            /// **Available** in OS X v10.5 and later.
            const POST_RENDER_ERROR = AudioUnitRenderActionFlags::UnitRenderAction_PostRenderError.0;
            /// If this flag is set, then checks that are done on the arguments provided to render
            /// are not performed. This can be useful to use to save computation time in situations
            /// where you are sure you are providing the correct arguments and structures to the
            /// various render calls.
            ///
            /// **Available** in OS X v10.7 and later.
            const DO_NOT_CHECK_RENDER_ARGS = AudioUnitRenderActionFlags::UnitRenderAction_DoNotCheckRenderArgs.0;
        }
    }

    /// A safe handle around the `AudioUnitRenderActionFlags` pointer provided by the render
    /// callback.
    ///
    /// This type lets a callback provide various hints to the audio unit.
    ///
    /// For example: if there is no audio to process, we can insert the `OUTPUT_IS_SILENCE` flag to
    /// indicate to the audio unit that the buffer does not need to be processed.
    pub struct Handle {
        ptr: *mut AudioUnitRenderActionFlags,
    }

    impl fmt::Debug for Handle {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            if self.ptr.is_null() {
                write!(f, "{:?}", self.ptr)
            } else {
                unsafe { write!(f, "{:?}", *self.ptr) }
            }
        }
    }

    impl Handle {
        /// Retrieve the current state of the `ActionFlags`.
        pub fn get(&self) -> ActionFlags {
            ActionFlags::from_bits_truncate(unsafe { *self.ptr }.0)
        }

        fn set(&mut self, flags: ActionFlags) {
            unsafe { (*self.ptr).0 = flags.bits() }
        }

        /// The raw value of the flags currently stored.
        pub fn bits(&self) -> u32 {
            self.get().bits()
        }

        /// Returns `true` if no flags are currently stored.
        pub fn is_empty(&self) -> bool {
            self.get().is_empty()
        }

        /// Returns `true` if all flags are currently stored.
        pub fn is_all(&self) -> bool {
            self.get().is_all()
        }

        /// Returns `true` if there are flags common to both `self` and `other`.
        pub fn intersects(&self, other: ActionFlags) -> bool {
            self.get().intersects(other)
        }

        /// Returns `true` if all of the flags in `other` are contained within `self`.
        pub fn contains(&self, other: ActionFlags) -> bool {
            self.get().contains(other)
        }

        /// Insert the specified flags in-place.
        pub fn insert(&mut self, other: ActionFlags) {
            let mut flags = self.get();
            flags.insert(other);
            self.set(flags);
        }

        /// Remove the specified flags in-place.
        pub fn remove(&mut self, other: ActionFlags) {
            let mut flags = self.get();
            flags.remove(other);
            self.set(flags);
        }

        /// Toggles the specified flags in-place.
        pub fn toggle(&mut self, other: ActionFlags) {
            let mut flags = self.get();
            flags.toggle(other);
            self.set(flags);
        }

        /// Wrap the given pointer with a `Handle`.
        pub fn from_ptr(ptr: *mut AudioUnitRenderActionFlags) -> Self {
            Handle { ptr }
        }
    }

    unsafe impl Send for Handle {}

    impl ::std::fmt::Display for ActionFlags {
        fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
            write!(
                f,
                "{:?}",
                match AudioUnitRenderActionFlags(self.bits()) {
                    AudioUnitRenderActionFlags::UnitRenderAction_PreRender => "PRE_RENDER",
                    AudioUnitRenderActionFlags::UnitRenderAction_PostRender => "POST_RENDER",
                    AudioUnitRenderActionFlags::UnitRenderAction_OutputIsSilence =>
                        "OUTPUT_IS_SILENCE",
                    AudioUnitRenderActionFlags::OfflineUnitRenderAction_Preflight =>
                        "OFFLINE_PREFLIGHT",
                    AudioUnitRenderActionFlags::OfflineUnitRenderAction_Render => "OFFLINE_RENDER",
                    AudioUnitRenderActionFlags::OfflineUnitRenderAction_Complete =>
                        "OFFLINE_COMPLETE",
                    AudioUnitRenderActionFlags::UnitRenderAction_PostRenderError =>
                        "POST_RENDER_ERROR",
                    AudioUnitRenderActionFlags::UnitRenderAction_DoNotCheckRenderArgs =>
                        "DO_NOT_CHECK_RENDER_ARGS",
                    _ => "<Unknown ActionFlags>",
                }
            )
        }
    }
}

impl AudioUnit {
    /// Pass a render callback (aka "Input Procedure") to the **AudioUnit**.
    pub fn set_render_callback<F, D>(&mut self, mut f: F) -> Result<(), Error>
    where
        F: FnMut(Args<D>) -> Result<(), ()> + 'static,
        D: Data,
    {
        // First, we'll retrieve the stream format so that we can ensure that the given callback
        // format matches the audio unit's format.
        let stream_format = self.output_stream_format()?;

        // If the stream format does not match, return an error indicating this.
        if !D::does_stream_format_match(&stream_format) {
            return Err(Error::RenderCallbackBufferFormatDoesNotMatchAudioUnitStreamFormat);
        }

        // Here, we call the given render callback function within a closure that matches the
        // arguments of the required coreaudio "input_proc".
        //
        // This allows us to take advantage of rust's type system and provide format-specific
        // `Args` types which can be checked at compile time.
        let input_proc_fn = move |io_action_flags: NonNull<AudioUnitRenderActionFlags>,
                                  in_time_stamp: NonNull<AudioTimeStamp>,
                                  in_bus_number: u32,
                                  in_number_frames: u32,
                                  io_data: *mut AudioBufferList|
              -> OSStatus {
            let args = unsafe {
                let data = D::from_input_proc_args(in_number_frames, io_data);
                let flags = action_flags::Handle::from_ptr(io_action_flags.as_ptr());
                Args {
                    data,
                    time_stamp: in_time_stamp.read(),
                    flags,
                    bus_number: in_bus_number,
                    num_frames: in_number_frames as usize,
                }
            };

            match f(args) {
                Ok(()) => 0,
                Err(()) => error::Error::Unspecified.as_os_status(),
            }
        };

        let input_proc_fn_wrapper = Box::new(InputProcFnWrapper {
            callback: Box::new(input_proc_fn),
        });

        // Setup render callback. Notice that we relinquish ownership of the Callback
        // here so that it can be used as the C render callback via a void pointer.
        // We do however store the *mut so that we can convert back to a Box<InputProcFnWrapper>
        // within our AudioUnit's Drop implementation (otherwise it would leak).
        let input_proc_fn_wrapper_ptr = Box::into_raw(input_proc_fn_wrapper) as *mut c_void;

        let render_callback = AURenderCallbackStruct {
            inputProc: Some(input_proc),
            inputProcRefCon: input_proc_fn_wrapper_ptr,
        };

        self.set_property(
            kAudioUnitProperty_SetRenderCallback,
            Scope::Input,
            Element::Output,
            Some(&render_callback),
        )?;

        self.free_render_callback();
        self.maybe_render_callback = Some(input_proc_fn_wrapper_ptr as *mut InputProcFnWrapper);
        Ok(())
    }

    /// Pass an input callback (aka "Input Procedure") to the **AudioUnit**.
    pub fn set_input_callback<F, D>(&mut self, mut f: F) -> Result<(), Error>
    where
        F: FnMut(Args<D>) -> Result<(), ()> + 'static,
        D: Data,
    {
        // First, we'll retrieve the stream format so that we can ensure that the given callback
        // format matches the audio unit's format.
        let stream_format = self.input_stream_format()?;

        // If the stream format does not match, return an error indicating this.
        if !D::does_stream_format_match(&stream_format) {
            return Err(Error::RenderCallbackBufferFormatDoesNotMatchAudioUnitStreamFormat);
        }

        // Interleaved or non-interleaved?
        let non_interleaved = stream_format
            .flags
            .contains(LinearPcmFlags::IS_NON_INTERLEAVED);

        // Pre-allocate a buffer list for input stream.
        //
        // First, get the current buffer size for pre-allocating the `AudioBuffer`s.
        #[cfg(target_os = "macos")]
        let buffer_frame_size: u32 = {
            let id = objc2_core_audio::kAudioDevicePropertyBufferFrameSize;
            let buffer_frame_size: u32 = self.get_property(id, Scope::Global, Element::Output)?;
            buffer_frame_size
        };
        #[cfg(any(target_os = "ios", target_os = "tvos", target_os = "visionos"))]
        let buffer_frame_size: u32 = {
            let id = objc2_audio_toolbox::kAudioSessionProperty_CurrentHardwareIOBufferDuration;
            let seconds: f32 = super::audio_session_get_property(id)?;
            let id = objc2_audio_toolbox::kAudioSessionProperty_CurrentHardwareSampleRate;
            let sample_rate: f64 = super::audio_session_get_property(id)?;
            (sample_rate * seconds as f64).round() as u32
        };
        let sample_bytes = stream_format.sample_format.size_in_bytes();
        let n_channels = stream_format.channels;
        if non_interleaved && n_channels > 1 {
            return Err(Error::NonInterleavedInputOnlySupportsMono);
        }

        // Patched (PATCHES.md): allocate room for the largest callback here,
        // off the audio thread; the callback only sets the length it uses.
        // `InputCallback` owns the buffer and frees it with its real capacity.
        let bytes_per_frame = sample_bytes * n_channels as usize;
        let max_frames = max_input_frames(self, buffer_frame_size) as usize;
        let buffer = InputBuffer::new(max_frames * bytes_per_frame, n_channels);
        let input_buffer = buffer.as_ref();
        let audio_buffer_list_ptr = input_buffer.list();
        let oversized = Arc::new(AtomicU64::new(0));
        let oversized_in_callback = Arc::clone(&oversized);

        // Here, we call the given input callback function within a closure that matches the
        // arguments of the required coreaudio "input_proc".
        //
        // This allows us to take advantage of rust's type system and provide format-specific
        // `Args` types which can be checked at compile time.
        let audio_unit = self.instance;
        let input_proc_fn = move |io_action_flags: NonNull<AudioUnitRenderActionFlags>,
                                  in_time_stamp: NonNull<AudioTimeStamp>,
                                  in_bus_number: u32,
                                  in_number_frames: u32,
                                  _io_data: *mut AudioBufferList|
              -> OSStatus {
            // Patched (PATCHES.md): no stream format query and no reallocation
            // here. The client format is fixed while the callback is set (the
            // unit converts the device's audio to it), and the buffer has room
            // for the largest callback; a larger one is counted and skipped.
            let data_byte_size = in_number_frames as usize * bytes_per_frame;
            // SAFETY: `InputCallback` keeps the buffer alive until the callback
            // is replaced or the unit is torn down, and only this callback uses
            // its list while the unit runs.
            if !unsafe { input_buffer.set_len(data_byte_size) } {
                oversized_in_callback.fetch_add(1, Ordering::Relaxed);
                return kAudioUnitErr_TooManyFramesToProcess;
            }

            unsafe {
                let status = AudioUnitRender(
                    audio_unit,
                    io_action_flags.as_ptr(),
                    in_time_stamp,
                    in_bus_number,
                    in_number_frames,
                    NonNull::new(audio_buffer_list_ptr).unwrap(),
                );
                if status != 0 {
                    return status;
                }
            }

            let args = unsafe {
                let data = D::from_input_proc_args(in_number_frames, audio_buffer_list_ptr);
                let flags = action_flags::Handle::from_ptr(io_action_flags.as_ptr());
                Args {
                    data,
                    time_stamp: in_time_stamp.read(),
                    flags,
                    bus_number: in_bus_number,
                    num_frames: in_number_frames as usize,
                }
            };

            match f(args) {
                Ok(()) => 0,
                Err(()) => error::Error::Unspecified.as_os_status(),
            }
        };

        let input_proc_fn_wrapper = Box::new(InputProcFnWrapper {
            callback: Box::new(input_proc_fn),
        });

        // Setup input callback. Notice that we relinquish ownership of the Callback
        // here so that it can be used as the C render callback via a void pointer.
        // We do however store the *mut so that we can convert back to a Box<InputProcFnWrapper>
        // within our AudioUnit's Drop implementation (otherwise it would leak).
        let input_proc_fn_wrapper_ptr = Box::into_raw(input_proc_fn_wrapper) as *mut c_void;

        let render_callback = AURenderCallbackStruct {
            inputProc: Some(input_proc),
            inputProcRefCon: input_proc_fn_wrapper_ptr,
        };

        self.set_property(
            kAudioOutputUnitProperty_SetInputCallback,
            Scope::Global,
            Element::Output,
            Some(&render_callback),
        )?;

        let input_callback = super::InputCallback {
            buffer,
            callback: input_proc_fn_wrapper_ptr as *mut InputProcFnWrapper,
            oversized,
        };
        self.free_input_callback();
        self.maybe_input_callback = Some(input_callback);
        Ok(())
    }

    /// Retrieves ownership over the render callback and returns it where it can be re-used or
    /// safely dropped.
    pub fn free_render_callback(&mut self) -> Option<Box<InputProcFnWrapper>> {
        if let Some(callback) = self.maybe_render_callback.take() {
            // Here, we transfer ownership of the callback back to the current scope so that it
            // is dropped and cleaned up. Without this line, we would leak the Boxed callback.
            let callback: Box<InputProcFnWrapper> = unsafe { Box::from_raw(callback) };
            return Some(callback);
        }
        None
    }

    /// Retrieves ownership over the input callback and returns it where it can be re-used or
    /// safely dropped.
    pub fn free_input_callback(&mut self) -> Option<Box<InputProcFnWrapper>> {
        if let Some(input_callback) = self.maybe_input_callback.take() {
            let super::InputCallback {
                buffer, callback, ..
            } = input_callback;
            // Patched (PATCHES.md): frees the buffer with the capacity it was
            // allocated with, not its current length.
            drop(buffer);
            // SAFETY: `callback` came from `Box::into_raw` in
            // `set_input_callback` and is taken out of `self` here, once.
            let callback: Box<InputProcFnWrapper> = unsafe { Box::from_raw(callback) };
            return Some(callback);
        }
        None
    }

    /// Patched (PATCHES.md): input callbacks skipped because they asked for
    /// more frames than the buffer allocated for them holds. Each one is a
    /// gap in the input; it should stay 0.
    pub fn input_callback_oversized_blocks(&self) -> u64 {
        self.maybe_input_callback
            .as_ref()
            .map_or(0, |callback| callback.oversized.load(Ordering::Relaxed))
    }
}

/// Callback procedure that will be called each time our audio_unit requests audio.
extern "C-unwind" fn input_proc(
    in_ref_con: NonNull<c_void>,
    io_action_flags: NonNull<AudioUnitRenderActionFlags>,
    in_time_stamp: NonNull<AudioTimeStamp>,
    in_bus_number: u32,
    in_number_frames: u32,
    io_data: *mut AudioBufferList,
) -> OSStatus {
    let wrapper = unsafe { in_ref_con.cast::<InputProcFnWrapper>().as_mut() };
    (wrapper.callback)(
        io_action_flags,
        in_time_stamp,
        in_bus_number,
        in_number_frames,
        io_data,
    )
}

/// Patched (PATCHES.md): the input buffer and callback across frame-count
/// changes, under an allocator that counts allocations and checks the size
/// the buffer is freed with.
#[cfg(test)]
mod patch_tests {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::ptr;
    use std::sync::atomic::AtomicUsize;

    struct Checked;

    thread_local! {
        /// Count this thread's allocations.
        static COUNTING: Cell<bool> = const { Cell::new(false) };
        static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    }
    /// The allocation to watch, and the size it was freed with.
    static WATCHED: AtomicUsize = AtomicUsize::new(0);
    static FREED_WITH: AtomicUsize = AtomicUsize::new(usize::MAX);

    unsafe impl GlobalAlloc for Checked {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let _ = COUNTING.try_with(|counting| {
                if counting.get() {
                    let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
                }
            });
            // SAFETY: forwarded unchanged.
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            if ptr as usize == WATCHED.load(Ordering::SeqCst) {
                FREED_WITH.store(layout.size(), Ordering::SeqCst);
            }
            // SAFETY: forwarded unchanged.
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let _ = COUNTING.try_with(|counting| {
                if counting.get() {
                    let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
                }
            });
            // SAFETY: forwarded unchanged.
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: Checked = Checked;

    /// Allocations `f` makes on this thread.
    fn allocations_in<R>(f: impl FnOnce() -> R) -> (R, usize) {
        ALLOCATIONS.with(|n| n.set(0));
        COUNTING.with(|c| c.set(true));
        let result = f();
        COUNTING.with(|c| c.set(false));
        (result, ALLOCATIONS.with(Cell::get))
    }

    fn reported(buffer: InputBufferRef) -> (*mut c_void, u32) {
        // SAFETY: the test owns the live buffer.
        let audio_buffer = unsafe { (*buffer.list()).mBuffers[0] };
        (audio_buffer.mData, audio_buffer.mDataByteSize)
    }

    /// Pure Rust, so it also runs under Miri, which checks the
    /// deallocation layout itself.
    #[test]
    fn the_buffer_grows_and_shrinks_in_place_and_frees_its_capacity() {
        let buffer = InputBuffer::new(4096, 1);
        let handle = buffer.as_ref();
        let data = handle.data as *mut c_void;
        let (fits, allocations) = allocations_in(|| {
            // SAFETY: the buffer is alive and only used here.
            [2048, 4096, 1024, 0, 4096, 256].map(|bytes| unsafe { handle.set_len(bytes) })
        });
        assert_eq!(fits, [true; 6]);
        assert_eq!(allocations, 0);
        assert_eq!(reported(handle), (data, 256));

        // SAFETY: as above.
        assert!(!unsafe { handle.set_len(4097) });
        assert_eq!(reported(handle), (data, 256), "unchanged");

        WATCHED.store(handle.data as usize, Ordering::SeqCst);
        drop(buffer);
        assert_eq!(FREED_WITH.swap(usize::MAX, Ordering::SeqCst), 4096);
        WATCHED.store(0, Ordering::SeqCst);
    }

    /// M2.3 criterion 2 (#77 B2): the real callback closure on a real HAL
    /// input unit. The unit is never initialized or started, so nothing
    /// records and `AudioUnitRender` returns `kAudioUnitErr_Uninitialized`
    /// after the callback has sized the buffer.
    #[cfg(target_os = "macos")]
    #[test]
    #[cfg_attr(miri, ignore = "calls CoreAudio")]
    fn the_input_callback_survives_growth_shrink_and_teardown() {
        use crate::audio_unit::macos_helpers::{
            audio_unit_from_device_id_uninitialized, get_default_device_id,
        };
        use objc2_audio_toolbox::kAudioUnitErr_Uninitialized;

        let Some(device) = get_default_device_id(true) else {
            eprintln!("no input device; skipped");
            return;
        };
        let mut unit = audio_unit_from_device_id_uninitialized(device, true).unwrap();
        // Interleaved stereo f32 at the device's rate, as cpal asks for.
        let format = super::super::StreamFormat {
            sample_rate: unit.input_stream_format().unwrap().sample_rate,
            sample_format: super::super::SampleFormat::F32,
            flags: LinearPcmFlags::IS_FLOAT | LinearPcmFlags::IS_PACKED,
            channels: 2,
        };
        unit.set_stream_format(format, Scope::Output, Element::Input)
            .unwrap();
        type Args = super::Args<data::Raw>;
        unit.set_input_callback(|_: Args| Ok(())).unwrap();

        let (buffer, wrapper) = {
            let callback = unit.maybe_input_callback.as_ref().unwrap();
            (callback.buffer.as_ref(), callback.callback)
        };
        let bytes_per_frame = 4 * 2;
        let max_frames = (buffer.capacity / bytes_per_frame) as u32;
        assert!(max_frames >= MIN_INPUT_BUFFER_FRAMES, "{}", max_frames);

        let mut flags: AudioUnitRenderActionFlags = AudioUnitRenderActionFlags(0);
        // SAFETY: a plain C struct; all zeros is a valid (empty) timestamp.
        let mut time: AudioTimeStamp = unsafe { mem::zeroed() };
        let mut call = |frames: u32| {
            // SAFETY: `wrapper` is the unit's live callback, called from one
            // thread, as CoreAudio would.
            let wrapper = unsafe { &mut *wrapper };
            (wrapper.callback)(
                NonNull::from(&mut flags),
                NonNull::from(&mut time),
                1,
                frames,
                ptr::null_mut(),
            )
        };

        let frames = [512, 1024, 256, max_frames, 1, 4096.min(max_frames), 128];
        let (statuses, allocations) = allocations_in(|| frames.map(&mut call));
        assert_eq!(statuses, [kAudioUnitErr_Uninitialized; 7]);
        assert_eq!(allocations, 0, "the callback allocated");
        assert_eq!(
            reported(buffer),
            (buffer.data as *mut c_void, 128 * bytes_per_frame as u32)
        );

        // Too big: counted and skipped, before rendering, without allocating.
        let (status, allocations) = allocations_in(|| call(max_frames + 1));
        assert_eq!(status, kAudioUnitErr_TooManyFramesToProcess);
        assert_eq!(allocations, 0);
        assert_eq!(unit.input_callback_oversized_blocks(), 1);

        WATCHED.store(buffer.data as usize, Ordering::SeqCst);
        drop(unit);
        assert_eq!(
            FREED_WITH.swap(usize::MAX, Ordering::SeqCst),
            buffer.capacity,
            "freed with its real capacity"
        );
        WATCHED.store(0, Ordering::SeqCst);
    }
}
