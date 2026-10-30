//! CLAP block-start transport projection, with source-defined availability.

use chassis_runtime::transport::{LoopRange, TimeSignature, TransportSnapshot, TransportValues};
use clack_plugin::{
    events::event_types::{TransportEvent, TransportFlags},
    plugin::PluginError,
};

pub(crate) fn map(transport: Option<&TransportEvent>) -> Result<TransportSnapshot, PluginError> {
    let Some(transport) = transport else {
        return Ok(TransportSnapshot::UNKNOWN);
    };
    let has = |flag| transport.flags.contains(flag);
    let beats = has(TransportFlags::HAS_BEATS_TIMELINE);
    let seconds = has(TransportFlags::HAS_SECONDS_TIMELINE);
    // Retain the existing boundary recovery: a malformed optional BPM is
    // unavailable, never an invalid core value or a fabricated default tempo.
    let tempo = has(TransportFlags::HAS_TEMPO)
        .then_some(transport.tempo)
        .filter(|tempo| tempo.is_finite() && *tempo > 0.0);
    // CLAP does not guarantee configured loop endpoints while looping is off.
    // Inactive storage is intentionally poisoned by the pinned validator, and
    // established CLAP wrappers also require this flag for range availability.
    let loop_active = has(TransportFlags::IS_LOOP_ACTIVE);
    // Check source ordering before lossy f64 projection can collapse distinct
    // fixed-point positions to the same floating value at large song times.
    if loop_active
        && ((beats && transport.loop_end_beats < transport.loop_start_beats)
            || (seconds && transport.loop_end_seconds < transport.loop_start_seconds))
    {
        return Err(PluginError::Message("Invalid CLAP transport loop range"));
    }
    TransportSnapshot::new(TransportValues {
        playing: Some(has(TransportFlags::IS_PLAYING)),
        recording: Some(has(TransportFlags::IS_RECORDING)),
        loop_active: Some(loop_active),
        within_pre_roll: Some(has(TransportFlags::IS_WITHIN_PRE_ROLL)),
        tempo_bpm: tempo,
        tempo_increment: tempo.map(|_| transport.tempo_inc),
        // CLAP steady_time is monotonic callback time, not project position.
        // Preserve its actual seconds timeline rather than rounding to samples.
        sample_position: None,
        seconds_position: seconds.then(|| f64::from(transport.song_pos_seconds)),
        beat_position: beats.then(|| f64::from(transport.song_pos_beats)),
        bar_start_beats: beats.then(|| f64::from(transport.bar_start)),
        bar_number: beats.then(|| i64::from(transport.bar_number)),
        time_signature: has(TransportFlags::HAS_TIME_SIGNATURE).then_some(TimeSignature {
            numerator: transport.time_signature_numerator,
            denominator: transport.time_signature_denominator,
        }),
        loop_beats: (loop_active && beats).then(|| LoopRange {
            start: f64::from(transport.loop_start_beats),
            end: f64::from(transport.loop_end_beats),
        }),
        loop_seconds: (loop_active && seconds).then(|| LoopRange {
            start: f64::from(transport.loop_start_seconds),
            end: f64::from(transport.loop_end_seconds),
        }),
    })
    .map_err(|_| PluginError::Message("Invalid CLAP transport metadata"))
}
