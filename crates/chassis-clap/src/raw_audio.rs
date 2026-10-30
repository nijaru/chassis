//! CLAP sample-pointer trust boundary, bypassing Clack's reference-first pairing.
//!
//! Clack's `Audio` guarantees live descriptor arrays. The host must additionally
//! provide readable, aligned pointer tables and initialized sample allocations
//! covering `frames_count` (writable for outputs), with metadata separate from
//! samples and no concurrent access to writable samples for the callback. We can
//! check null/alignment/address extents and sample aliases, not allocation
//! provenance, actual allocation lengths, initialization, or host concurrency.
//!
//! Tables are read as raw pointers only. All sample aliases are classified before
//! ANY sample reference exists. Input/input overlap is allowed; every overlap
//! involving an output must be its exact reciprocal input at the same channel.
//! Scratch is activation-owned. Traversal borrows the source exclusively, yields
//! each descriptor once, and cannot outlive either Audio or scratch. Reiteration
//! requires the previous iterator AND its yielded references to have expired.
//! Storage is O(C); whole-topology validation is O(C²), where C is bounded by
//! the activation's mapped channel count, never a host-supplied work budget.
#![allow(unsafe_code)]

use crate::audio::ClapProcessSlot;
use chassis_runtime::buffer::{InputEndpoint, OutputEndpoint};
use clack_plugin::prelude::{Audio, ChannelPair, PluginError};
use core::{marker::PhantomData, mem, ptr::NonNull, slice};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClapSamplePrecision {
    F32,
    F64,
}

pub(crate) trait ClapSample: Copy {
    const PRECISION: ClapSamplePrecision;
    fn table(f32: *mut *mut f32, f64: *mut *mut f64) -> *mut *mut Self;
}
impl ClapSample for f32 {
    const PRECISION: ClapSamplePrecision = ClapSamplePrecision::F32;
    fn table(f32: *mut *mut f32, _: *mut *mut f64) -> *mut *mut Self {
        f32
    }
}
impl ClapSample for f64 {
    const PRECISION: ClapSamplePrecision = ClapSamplePrecision::F64;
    fn table(_: *mut *mut f32, f64: *mut *mut f64) -> *mut *mut Self {
        f64
    }
}

#[derive(Clone, Copy)]
struct Descriptor {
    input: Option<*mut ()>,
    output: Option<*mut ()>,
    input_endpoint: Option<InputEndpoint>,
    output_endpoint: Option<OutputEndpoint>,
}

pub(crate) struct AudioScratch {
    channels: Vec<Descriptor>,
}
// SAFETY: Scratch owns only storage, never sample allocations. Its stale pointers
// are never dereferenced outside RawSource, whose construction replaces them
// using a live Audio. A live source exclusively borrows scratch, preventing it
// from moving to another thread while sample references are accessible.
unsafe impl Send for AudioScratch {}
impl AudioScratch {
    pub(crate) fn new(slots: &[ClapProcessSlot]) -> Result<Self, PluginError> {
        let mut count = 0usize;
        for slot in slots {
            for port in [slot.input, slot.output].into_iter().flatten() {
                count = count
                    .checked_add(
                        usize::try_from(port.layout.channel_count()).map_err(|_| invalid())?,
                    )
                    .ok_or_else(invalid)?;
            }
        }
        let mut channels = Vec::new();
        channels
            .try_reserve_exact(count)
            .map_err(PluginError::from)?;
        Ok(Self { channels })
    }
}

fn invalid() -> PluginError {
    PluginError::Message("Invalid CLAP audio buffer topology")
}

pub(crate) fn sample_precision(
    audio: &mut Audio<'_>,
    slots: &[ClapProcessSlot],
) -> Result<ClapSamplePrecision, PluginError> {
    let (inputs, outputs) = audio.raw_buffers();
    if inputs.len() != slots.iter().filter(|s| s.input.is_some()).count()
        || outputs.len() != slots.iter().filter(|s| s.output.is_some()).count()
    {
        return Err(invalid());
    }
    // Event-only components have no wire sample representation. Dispatch their
    // required f32 process capability without inventing audio buffers; the
    // descriptor cardinality checks above still reject unexpected host ports.
    if slots.is_empty() {
        return Ok(ClapSamplePrecision::F32);
    }
    let mut precision = None;
    for (index, slot) in slots.iter().enumerate() {
        for (buffer, port) in [
            (inputs.get(index), slot.input),
            (outputs.get(index), slot.output),
        ] {
            match (buffer, port) {
                (Some(buffer), Some(port)) => {
                    if buffer.channel_count != port.layout.channel_count() {
                        return Err(invalid());
                    }
                    let current = match (buffer.data32.is_null(), buffer.data64.is_null()) {
                        (false, true) => ClapSamplePrecision::F32,
                        (true, false) => ClapSamplePrecision::F64,
                        _ => return Err(invalid()),
                    };
                    if precision.replace(current).is_some_and(|p| p != current) {
                        return Err(invalid());
                    }
                }
                (None, None) => {}
                _ => return Err(invalid()),
            }
        }
    }
    precision.ok_or_else(invalid)
}

// Address arithmetic is only for validation; retain the original pointer's
// provenance for reads and slice construction. Zero-length samples use dangling.
fn extent<T>(pointer: *mut T, count: usize) -> Result<(usize, usize), PluginError> {
    let start = pointer.addr();
    let bytes = count.checked_mul(mem::size_of::<T>()).ok_or_else(invalid)?;
    if pointer.is_null() || !pointer.is_aligned() || bytes > isize::MAX as usize {
        return Err(invalid());
    }
    Ok((start, start.checked_add(bytes).ok_or_else(invalid)?))
}

fn read_table<S: ClapSample>(
    table: *mut *mut S,
    channel: usize,
    count: usize,
    frames: usize,
) -> Result<*mut (), PluginError> {
    extent(table, count)?;
    // SAFETY: Audio's host contract supplies a readable table of count pointers;
    // count was checked against the activation layout, and channel < count.
    let pointer = unsafe { table.add(channel).read() };
    if frames != 0 {
        extent(pointer, frames)?;
    }
    Ok(pointer.cast())
}

pub(crate) struct RawSource<'audio, 'scratch, S> {
    audio: Audio<'audio>,
    scratch: &'scratch mut AudioScratch,
    frames: usize,
    _sample: PhantomData<S>,
}
impl<'audio, 'scratch, S: ClapSample> RawSource<'audio, 'scratch, S> {
    pub(crate) fn new(
        mut audio: Audio<'audio>,
        slots: &[ClapProcessSlot],
        scratch: &'scratch mut AudioScratch,
    ) -> Result<Self, PluginError> {
        if sample_precision(&mut audio, slots)? != S::PRECISION {
            return Err(invalid());
        }
        let frames = usize::try_from(audio.frames_count()).map_err(|_| invalid())?;
        scratch.channels.clear(); // descriptors contain no references or owning objects
        let (inputs, outputs) = audio.raw_buffers();
        for (index, slot) in slots.iter().enumerate() {
            let input_count = slot.input.map_or(0, |p| p.layout.channel_count() as usize);
            let output_count = slot.output.map_or(0, |p| p.layout.channel_count() as usize);
            for channel in 0..input_count.max(output_count) {
                let input = if channel < input_count {
                    let buffer = &inputs[index];
                    Some(read_table::<S>(
                        S::table(buffer.data32, buffer.data64),
                        channel,
                        input_count,
                        frames,
                    )?)
                } else {
                    None
                };
                let output = if channel < output_count {
                    let buffer = &outputs[index];
                    Some(read_table::<S>(
                        S::table(buffer.data32, buffer.data64),
                        channel,
                        output_count,
                        frames,
                    )?)
                } else {
                    None
                };
                let channel = u32::try_from(channel).map_err(|_| invalid())?;
                let input_endpoint =
                    input.and_then(|_| slot.input.map(|p| InputEndpoint::new(p.index, channel)));
                let output_endpoint =
                    output.and_then(|_| slot.output.map(|p| OutputEndpoint::new(p.index, channel)));
                if slot.paired {
                    if input.is_none() || output.is_none() {
                        return Err(invalid());
                    }
                    push(
                        scratch,
                        Descriptor {
                            input,
                            output,
                            input_endpoint,
                            output_endpoint,
                        },
                    )?;
                } else {
                    if input.is_some() {
                        push(
                            scratch,
                            Descriptor {
                                input,
                                output: None,
                                input_endpoint,
                                output_endpoint: None,
                            },
                        )?;
                    }
                    if output.is_some() {
                        push(
                            scratch,
                            Descriptor {
                                input: None,
                                output,
                                input_endpoint: None,
                                output_endpoint,
                            },
                        )?;
                    }
                }
            }
        }
        validate_aliases::<S>(&scratch.channels, frames)?;
        // DSP has no constant-output contract. Do not retain host descriptor
        // flags when the product may have written varying samples.
        for output in audio.raw_output_buffers() {
            output.constant_mask = 0;
        }
        Ok(Self {
            audio,
            scratch,
            frames,
            _sample: PhantomData,
        })
    }
    pub(crate) fn frame_count(&self) -> u32 {
        self.audio.frames_count()
    }
    pub(crate) fn channels(&mut self) -> RawChannels<'_, S> {
        RawChannels {
            descriptors: self.scratch.channels.iter(),
            frames: self.frames,
            _borrow: PhantomData,
        }
    }
}
fn push(scratch: &mut AudioScratch, descriptor: Descriptor) -> Result<(), PluginError> {
    if scratch.channels.len() == scratch.channels.capacity() {
        return Err(invalid());
    }
    scratch.channels.push(descriptor);
    Ok(())
}

fn validate_aliases<S>(channels: &[Descriptor], frames: usize) -> Result<(), PluginError> {
    for (index, descriptor) in channels.iter().enumerate() {
        let Some(output) = descriptor.output else {
            continue;
        };
        // Even an output-only callback must validate its extent.
        if frames == 0 {
            continue;
        }
        let (start, end) = extent(output.cast::<S>(), frames)?;
        for (other_index, other) in channels.iter().enumerate() {
            for (pointer, writable) in [(other.input, false), (other.output, true)] {
                let Some(pointer) = pointer else { continue };
                if writable && index == other_index {
                    continue;
                }
                let (other_start, other_end) = extent(pointer.cast::<S>(), frames)?;
                if start < other_end && other_start < end {
                    if !writable && index == other_index && start == other_start && end == other_end
                    {
                        continue;
                    }
                    return Err(invalid());
                }
            }
        }
    }
    Ok(())
}

pub(crate) struct RawChannels<'a, S> {
    descriptors: slice::Iter<'a, Descriptor>,
    frames: usize,
    // Exclusive source borrow, including every yielded sample reference.
    _borrow: PhantomData<&'a mut [S]>,
}
impl<'a, S: ClapSample> Iterator for RawChannels<'a, S> {
    type Item = (
        ChannelPair<'a, S>,
        Option<InputEndpoint>,
        Option<OutputEndpoint>,
    );
    fn next(&mut self) -> Option<Self::Item> {
        let descriptor = self.descriptors.next()?;
        let normalize = |p: *mut ()| {
            if self.frames == 0 {
                NonNull::<S>::dangling().as_ptr()
            } else {
                p.cast::<S>()
            }
        };
        // SAFETY: RawSource checked every pointer extent/alignment and the whole
        // topology before creating any samples (empty views use dangling).
        // Paired descriptors come only from validated reciprocal setup slots.
        // Host supplies live initialized
        // allocations and exclusivity. Each descriptor is yielded once. Only
        // shared inputs may overlap; exact in-place creates JUST the output
        // mutable reference. Source/Audio are exclusively borrowed for 'a.
        let pair = unsafe {
            match (descriptor.input, descriptor.output) {
                (Some(i), Some(o)) if i == o => {
                    ChannelPair::InPlace(slice::from_raw_parts_mut(normalize(o), self.frames))
                }
                (Some(i), Some(o)) => ChannelPair::InputOutput(
                    slice::from_raw_parts(normalize(i), self.frames),
                    slice::from_raw_parts_mut(normalize(o), self.frames),
                ),
                (Some(i), None) => {
                    ChannelPair::InputOnly(slice::from_raw_parts(normalize(i), self.frames))
                }
                (None, Some(o)) => {
                    ChannelPair::OutputOnly(slice::from_raw_parts_mut(normalize(o), self.frames))
                }
                (None, None) => unreachable!("descriptor has at least one endpoint"),
            }
        };
        Some((pair, descriptor.input_endpoint, descriptor.output_endpoint))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::ClapProcessPort;
    use chassis_runtime::audio::{AudioPortIndex, ChannelLayout};
    use clack_host::prelude::*;

    #[test]
    fn zero_audio_keeps_callback_frames_without_requiring_sample_storage() {
        let mut outputs = [];
        // SAFETY: both descriptor arrays are empty, so there are no pointer
        // tables or sample allocations to validate or borrow.
        let mut audio = unsafe { Audio::from_raw_buffers(&[], &mut outputs, 8) };
        assert!(sample_precision(&mut audio, &slots(false)).is_err());
        let mut scratch = AudioScratch::new(&[]).unwrap();
        let mut source = RawSource::<f32>::new(audio, &[], &mut scratch).unwrap();
        assert_eq!(source.frame_count(), 8);
        assert_eq!(source.channels().count(), 0);
    }

    fn slots(paired: bool) -> [ClapProcessSlot; 2] {
        core::array::from_fn(|index| {
            let index = u32::try_from(index).unwrap();
            ClapProcessSlot {
                input: Some(ClapProcessPort {
                    index: AudioPortIndex::new(index * 2),
                    layout: ChannelLayout::Mono,
                }),
                output: Some(ClapProcessPort {
                    index: AudioPortIndex::new(index * 2 + 1),
                    layout: ChannelLayout::Mono,
                }),
                paired,
            }
        })
    }

    // Run the real raw Audio entry with independently owned pointer tables.
    // No sample refs are manufactured for aliased topology by the host helper.
    fn callback<S: TestSample>(
        inputs: [*mut S; 2],
        outputs: [*mut S; 2],
        frames: u32,
        paired: bool,
        accepts: bool,
    ) {
        // Same samples arranged as two mono ports and as one stereo port:
        // cross-port and cross-channel overlaps must obey the same policy.
        callback_shape(inputs, outputs, frames, paired, accepts, false);
        callback_shape(inputs, outputs, frames, paired, accepts, true);
    }

    fn callback_shape<S: TestSample>(
        inputs: [*mut S; 2],
        outputs: [*mut S; 2],
        frames: u32,
        paired: bool,
        accepts: bool,
        stereo: bool,
    ) {
        let mut ports = AudioPorts::with_capacity(1, 1);
        let mut dummy = [0f32; 1];
        let prototype = ports
            .with_input_buffers([AudioPortBuffer {
                channels: AudioPortBufferType::f32_input_only([InputChannel::variable(&mut dummy)]),
                latency: 0,
            }])
            .as_raw_buffers()[0];
        let mut input_tables = [inputs; 2];
        let mut output_tables = [outputs; 2];
        let mut raw_inputs = [prototype; 2];
        let mut raw_outputs = [prototype; 2];
        let port_count = if stereo { 1 } else { 2 };
        for index in 0..port_count {
            let offset = if stereo { 0 } else { index };
            for (buffer, table) in [
                (
                    &mut raw_inputs[index],
                    input_tables[index].as_mut_ptr().wrapping_add(offset),
                ),
                (
                    &mut raw_outputs[index],
                    output_tables[index].as_mut_ptr().wrapping_add(offset),
                ),
            ] {
                buffer.channel_count = if stereo { 2 } else { 1 };
                buffer.data32 = core::ptr::null_mut();
                buffer.data64 = core::ptr::null_mut();
                match S::PRECISION {
                    ClapSamplePrecision::F32 => buffer.data32 = table.cast(),
                    ClapSamplePrecision::F64 => buffer.data64 = table.cast(),
                }
            }
        }
        if inputs == outputs {
            // Legal in-place hosts may share the pointer table itself too.
            // Reading raw entries must not create conflicting table references.
            for index in 0..port_count {
                raw_outputs[index].data32 = raw_inputs[index].data32;
                raw_outputs[index].data64 = raw_inputs[index].data64;
            }
        }
        let mut slots = slots(paired);
        if stereo {
            slots[0].input.as_mut().unwrap().layout = ChannelLayout::Stereo;
            slots[0].output.as_mut().unwrap().layout = ChannelLayout::Stereo;
        }
        let slots = &slots[..port_count];
        let mut scratch = AudioScratch::new(slots).unwrap();
        // SAFETY: descriptors and pointer tables remain live for the call.
        // Tests provide initialized allocations of sufficient extent. Rejected
        // null/misaligned/overlapping topology is not dereferenced as samples.
        let audio = unsafe {
            Audio::from_raw_buffers(
                &raw_inputs[..port_count],
                &mut raw_outputs[..port_count],
                frames,
            )
        };
        let result = RawSource::<S>::new(audio, slots, &mut scratch);
        assert_eq!(result.is_ok(), accepts);
        if let Ok(mut source) = result {
            // Retain all channel views at once, then reiterate after dropping them.
            // This protects the stronger iterator contract, not just local pairing.
            for _ in 0..2 {
                let channels: Vec<_> = source.channels().collect();
                assert_eq!(channels.len(), if paired { 2 } else { 4 });
                for (pair, _, _) in channels {
                    match pair {
                        ChannelPair::InPlace(o) | ChannelPair::OutputOnly(o) => {
                            o.fill(S::table_value());
                        }
                        ChannelPair::InputOutput(i, o) => o.copy_from_slice(i),
                        ChannelPair::InputOnly(i) => assert_eq!(i.len(), frames as usize),
                    }
                }
            }
        }
    }

    trait TestSample: ClapSample {
        fn table_value() -> Self;
    }
    impl TestSample for f32 {
        fn table_value() -> Self {
            7.0
        }
    }
    impl TestSample for f64 {
        fn table_value() -> Self {
            7.0
        }
    }

    fn topology_cases<S: TestSample>() {
        let mut storage = [S::table_value(); 16];
        let base = storage.as_mut_ptr();
        // SAFETY: all offsets are within the live 16-sample allocation.
        let [first, second, third, fourth, partial] =
            unsafe { [base, base.add(4), base.add(8), base.add(12), base.add(1)] };
        callback([first, third], [second, fourth], 4, true, true); // separate paired channels
        callback([first, third], [first, third], 4, true, true); // exact in-place
        callback([first, first], [second, fourth], 4, true, true); // shared inputs, separate outputs
        callback([first, partial], [third, fourth], 4, false, true); // partial input/input
        callback([first, third], [partial, fourth], 4, true, false); // partial pair
        callback([first, third], [third, fourth], 4, true, false); // cross-port output/input
        callback([first, third], [second, second], 4, true, false); // duplicate writable
        callback([first, third], [first, third], 4, false, false); // undeclared exact alias
        callback([first, first], [first, fourth], 4, true, false); // extra input aliases in-place
        callback([first, third], [second, partial], 4, true, false); // partial writable
        callback(
            [core::ptr::null_mut::<S>(); 2],
            [core::ptr::null_mut::<S>(); 2],
            0,
            true,
            true,
        );
        callback(
            [core::ptr::null_mut(), third],
            [second, fourth],
            4,
            true,
            false,
        );
        callback(
            [first.cast::<u8>().wrapping_add(1).cast(), third],
            [second, fourth],
            4,
            true,
            false,
        );
    }
    #[test]
    fn raw_boundary_f32_topologies() {
        topology_cases::<f32>();
    }
    #[test]
    fn raw_boundary_f64_topologies() {
        topology_cases::<f64>();
    }

    #[test]
    fn extent_rejects_unrepresentable_ranges_without_dereference() {
        let pointer = NonNull::<f64>::dangling().as_ptr();
        assert!(extent(pointer, usize::MAX).is_err());
        let near_end = core::ptr::without_provenance_mut::<f64>(usize::MAX - 7);
        assert!(extent(near_end, 1).is_err());
    }
}
