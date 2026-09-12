//! Setup-time CLAP audio-port projection.
//!
//! Stable Chassis port keys remain the product identity. CLAP IDs are supplied
//! explicitly by the deployment mapping, while CLAP port indices are dense
//! direction-local setup details. Mapping and validation run outside the audio
//! callback and may allocate.

use core::{cmp::Ordering, fmt};
use std::vec::Vec;

use chassis_core::audio::{
    AudioIoConfiguration, AudioPortDescriptor, AudioPortIndex, ChannelLayout, ConfiguredAudioPort,
    MAIN_INPUT, MAIN_OUTPUT, PortDirection, PortKey, PortRole, SIDECHAIN_INPUT, audio_port_index,
};
use clack_extensions::audio_ports::{AudioPortFlags, AudioPortInfo, AudioPortType};
use clack_plugin::prelude::ClapId;

/// Stable CLAP projection metadata for one Chassis audio port.
///
/// The numeric ID is a compatibility identity within one CLAP port direction;
/// it must not be derived from declaration order. The layout is the currently
/// supported CLAP projection for this adapter slice. `in_place_pair` names the
/// opposite-direction Chassis port that may alias this port in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClapAudioPort {
    /// Stable Chassis port key being projected.
    pub key: PortKey,
    /// Stable CLAP numeric ID. `u32::MAX` is invalid in CLAP.
    pub id: u32,
    /// CLAP-visible channel layout for this port.
    pub layout: ChannelLayout,
    /// Opposite-direction Chassis port that supports exact in-place pairing.
    pub in_place_pair: Option<PortKey>,
}

impl ClapAudioPort {
    /// Construct explicit CLAP projection metadata for one Chassis port.
    #[must_use]
    pub const fn new(
        key: PortKey,
        id: u32,
        layout: ChannelLayout,
        in_place_pair: Option<PortKey>,
    ) -> Self {
        Self {
            key,
            id,
            layout,
            in_place_pair,
        }
    }
}

/// Stable default CLAP mapping for the conventional effect schema.
///
/// Main input/output intentionally share ID 0 because CLAP IDs are scoped by
/// direction. The optional sidechain is input ID 1 and has no in-place pair.
pub const DEFAULT_CLAP_AUDIO_PORTS: [ClapAudioPort; 3] = [
    ClapAudioPort::new(MAIN_INPUT, 0, ChannelLayout::Stereo, Some(MAIN_OUTPUT)),
    ClapAudioPort::new(MAIN_OUTPUT, 0, ChannelLayout::Stereo, Some(MAIN_INPUT)),
    ClapAudioPort::new(SIDECHAIN_INPUT, 1, ChannelLayout::Stereo, None),
];

#[derive(Debug, Clone, Copy)]
struct PortBinding {
    descriptor: AudioPortDescriptor,
    index: AudioPortIndex,
    mapping: ClapAudioPort,
    in_place_pair_id: Option<u32>,
}

impl PortBinding {
    fn info(self, supports_f64: bool) -> AudioPortInfo<'static> {
        let mut flags = if self.descriptor.role == PortRole::Main {
            AudioPortFlags::IS_MAIN
        } else {
            AudioPortFlags::empty()
        };
        if supports_f64 {
            flags |= AudioPortFlags::SUPPORTS_64BITS | AudioPortFlags::REQUIRES_COMMON_SAMPLE_SIZE;
        }
        AudioPortInfo {
            id: ClapId::new(self.mapping.id),
            name: self.descriptor.name.as_bytes(),
            channel_count: self.mapping.layout.channel_count(),
            flags,
            port_type: AudioPortType::from_channel_count(self.mapping.layout.channel_count()),
            in_place_pair: self.in_place_pair_id.map(ClapId::new),
        }
    }
}

/// One direction-specific port projected into a dense CLAP process slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClapProcessPort {
    /// Stable setup identity retained only while process endpoint consumers migrate.
    pub(crate) key: PortKey,
    /// Dense Chassis schema-local audio-port identity for callback use.
    pub(crate) index: AudioPortIndex,
    pub(crate) layout: ChannelLayout,
}

/// Setup-qualified semantic meaning of one dense CLAP process slot.
///
/// CLAP exposes input/output buffer arrays by dense direction-local index. A
/// slot can therefore contain two unrelated ports even when both arrays have an
/// entry at the same index. `paired` is true only when the setup mapping says
/// those exact ports are reciprocal in-place partners; otherwise the callback
/// must preserve them as independent semantic channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClapProcessSlot {
    pub(crate) input: Option<ClapProcessPort>,
    pub(crate) output: Option<ClapProcessPort>,
    pub(crate) paired: bool,
}

/// Validated setup-owned CLAP audio mapping for one component instance.
pub(crate) struct ClapAudioConfiguration {
    inputs: Vec<PortBinding>,
    outputs: Vec<PortBinding>,
    configured: Vec<ConfiguredAudioPort>,
    process_slots: Vec<ClapProcessSlot>,
    supports_f64: bool,
}

impl ClapAudioConfiguration {
    pub(crate) fn new(
        descriptors: &[AudioPortDescriptor],
        mapping: &[ClapAudioPort],
    ) -> Result<Self, AudioMappingError> {
        validate_mapping_identity(descriptors, mapping)?;
        let configured = configured_ports(descriptors, mapping)?;
        AudioIoConfiguration::new(&configured)
            .validate(descriptors)
            .map_err(AudioMappingError::InvalidConfiguration)?;

        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        inputs
            .try_reserve_exact(mapping.len())
            .map_err(|_| AudioMappingError::AllocationFailed)?;
        outputs
            .try_reserve_exact(mapping.len())
            .map_err(|_| AudioMappingError::AllocationFailed)?;

        for mapped in mapping {
            let binding = port_binding(descriptors, mapping, mapped)?;
            match binding.descriptor.direction {
                PortDirection::Input => inputs.push(binding),
                PortDirection::Output => outputs.push(binding),
            }
        }

        order_direction(&mut inputs)?;
        order_direction(&mut outputs)?;
        validate_direction_ids(&inputs)?;
        validate_direction_ids(&outputs)?;
        validate_pair_alignment(&inputs, &outputs)?;
        let process_slots = process_slots(&inputs, &outputs)?;

        Ok(Self {
            inputs,
            outputs,
            configured,
            process_slots,
            supports_f64: false,
        })
    }

    pub(crate) fn with_f64_support(mut self) -> Self {
        self.supports_f64 = true;
        self
    }

    pub(crate) fn count(&self, direction: PortDirection) -> u32 {
        let length = match direction {
            PortDirection::Input => self.inputs.len(),
            PortDirection::Output => self.outputs.len(),
        };
        u32::try_from(length).unwrap_or(u32::MAX)
    }

    pub(crate) fn info(
        &self,
        index: u32,
        direction: PortDirection,
    ) -> Option<AudioPortInfo<'static>> {
        let index = usize::try_from(index).ok()?;
        let ports = match direction {
            PortDirection::Input => &self.inputs,
            PortDirection::Output => &self.outputs,
        };
        ports
            .get(index)
            .copied()
            .map(|binding| binding.info(self.supports_f64))
    }

    pub(crate) fn audio_io(&self) -> AudioIoConfiguration<'_> {
        AudioIoConfiguration::new(&self.configured)
    }

    pub(crate) fn process_slots(&self) -> &[ClapProcessSlot] {
        &self.process_slots
    }
}

fn validate_mapping_identity(
    descriptors: &[AudioPortDescriptor],
    mapping: &[ClapAudioPort],
) -> Result<(), AudioMappingError> {
    for (index, mapped) in mapping.iter().enumerate() {
        if mapped.id == u32::MAX {
            return Err(AudioMappingError::InvalidClapId(mapped.key));
        }
        if mapping[..index]
            .iter()
            .any(|previous| previous.key == mapped.key)
        {
            return Err(AudioMappingError::DuplicateMappedPort(mapped.key));
        }
        if !descriptors
            .iter()
            .any(|descriptor| descriptor.key == mapped.key)
        {
            return Err(AudioMappingError::UnknownMappedPort(mapped.key));
        }
    }
    Ok(())
}

fn configured_ports(
    descriptors: &[AudioPortDescriptor],
    mapping: &[ClapAudioPort],
) -> Result<Vec<ConfiguredAudioPort>, AudioMappingError> {
    let mut configured = Vec::new();
    configured
        .try_reserve_exact(mapping.len())
        .map_err(|_| AudioMappingError::AllocationFailed)?;
    for descriptor in descriptors {
        if let Some(mapped) = mapping
            .iter()
            .find(|candidate| candidate.key == descriptor.key)
        {
            configured.push(ConfiguredAudioPort {
                key: mapped.key,
                layout: mapped.layout,
            });
        }
    }
    Ok(configured)
}

fn port_binding(
    descriptors: &[AudioPortDescriptor],
    mapping: &[ClapAudioPort],
    mapped: &ClapAudioPort,
) -> Result<PortBinding, AudioMappingError> {
    let index = audio_port_index(descriptors, mapped.key)
        .ok_or(AudioMappingError::UnknownMappedPort(mapped.key))?;
    let descriptor = descriptors
        .get(usize::try_from(index.get()).map_err(|_| AudioMappingError::IndexNotRepresentable)?)
        .copied()
        .ok_or(AudioMappingError::UnknownMappedPort(mapped.key))?;
    let in_place_pair_id = in_place_pair_id(descriptors, mapping, descriptor, *mapped)?;
    Ok(PortBinding {
        descriptor,
        index,
        mapping: *mapped,
        in_place_pair_id,
    })
}

fn in_place_pair_id(
    descriptors: &[AudioPortDescriptor],
    mapping: &[ClapAudioPort],
    descriptor: AudioPortDescriptor,
    mapped: ClapAudioPort,
) -> Result<Option<u32>, AudioMappingError> {
    let Some(pair_key) = mapped.in_place_pair else {
        return Ok(None);
    };
    let pair_descriptor = descriptors
        .iter()
        .copied()
        .find(|candidate| candidate.key == pair_key)
        .ok_or(AudioMappingError::UnknownInPlacePair {
            port: mapped.key,
            pair: pair_key,
        })?;
    if pair_descriptor.direction == descriptor.direction {
        return Err(AudioMappingError::InPlacePairSameDirection {
            port: mapped.key,
            pair: pair_key,
        });
    }
    let pair = mapping
        .iter()
        .copied()
        .find(|candidate| candidate.key == pair_key)
        .ok_or(AudioMappingError::InactiveInPlacePair {
            port: mapped.key,
            pair: pair_key,
        })?;
    if pair.layout.channel_count() != mapped.layout.channel_count() {
        return Err(AudioMappingError::InPlacePairChannelMismatch {
            port: mapped.key,
            pair: pair_key,
        });
    }
    if pair.in_place_pair != Some(mapped.key) {
        return Err(AudioMappingError::NonReciprocalInPlacePair {
            port: mapped.key,
            pair: pair_key,
        });
    }
    Ok(Some(pair.id))
}

fn order_direction(ports: &mut [PortBinding]) -> Result<(), AudioMappingError> {
    let main_count = ports
        .iter()
        .filter(|binding| binding.descriptor.role == PortRole::Main)
        .count();
    if main_count > 1 {
        return Err(AudioMappingError::MultipleMainPorts);
    }
    ports.sort_by(compare_process_order);
    Ok(())
}

fn compare_process_order(left: &PortBinding, right: &PortBinding) -> Ordering {
    process_rank(left)
        .cmp(&process_rank(right))
        .then_with(|| process_pair_key(left).cmp(process_pair_key(right)))
        .then_with(|| left.mapping.key.as_str().cmp(right.mapping.key.as_str()))
}

fn process_rank(binding: &PortBinding) -> u8 {
    if binding.descriptor.role == PortRole::Main {
        0
    } else if binding.mapping.in_place_pair.is_some() {
        1
    } else {
        2
    }
}

fn process_pair_key(binding: &PortBinding) -> &str {
    match (binding.descriptor.direction, binding.mapping.in_place_pair) {
        (PortDirection::Output, Some(input)) => input.as_str(),
        (PortDirection::Input, Some(_)) | (_, None) => binding.mapping.key.as_str(),
    }
}

fn validate_direction_ids(ports: &[PortBinding]) -> Result<(), AudioMappingError> {
    for (index, binding) in ports.iter().enumerate() {
        if ports[..index]
            .iter()
            .any(|previous| previous.mapping.id == binding.mapping.id)
        {
            return Err(AudioMappingError::DuplicateClapId(binding.mapping.id));
        }
    }
    Ok(())
}

fn validate_pair_alignment(
    inputs: &[PortBinding],
    outputs: &[PortBinding],
) -> Result<(), AudioMappingError> {
    for (index, input) in inputs.iter().enumerate() {
        let Some(pair) = input.mapping.in_place_pair else {
            continue;
        };
        if outputs.get(index).map(|output| output.mapping.key) != Some(pair) {
            return Err(AudioMappingError::InPlacePairIndexMismatch {
                port: input.mapping.key,
                pair,
            });
        }
    }
    for (index, output) in outputs.iter().enumerate() {
        let Some(pair) = output.mapping.in_place_pair else {
            continue;
        };
        if inputs.get(index).map(|input| input.mapping.key) != Some(pair) {
            return Err(AudioMappingError::InPlacePairIndexMismatch {
                port: output.mapping.key,
                pair,
            });
        }
    }
    Ok(())
}

fn process_slots(
    inputs: &[PortBinding],
    outputs: &[PortBinding],
) -> Result<Vec<ClapProcessSlot>, AudioMappingError> {
    let slot_count = inputs.len().max(outputs.len());
    let mut slots = Vec::new();
    slots
        .try_reserve_exact(slot_count)
        .map_err(|_| AudioMappingError::AllocationFailed)?;
    for index in 0..slot_count {
        let input_binding = inputs.get(index);
        let output_binding = outputs.get(index);
        let input = input_binding.map(process_port);
        let output = output_binding.map(process_port);
        let paired = match (input_binding, output_binding) {
            (Some(input), Some(output)) => {
                input.mapping.in_place_pair == Some(output.mapping.key)
                    && output.mapping.in_place_pair == Some(input.mapping.key)
            }
            _ => false,
        };
        slots.push(ClapProcessSlot {
            input,
            output,
            paired,
        });
    }
    Ok(slots)
}

fn process_port(binding: &PortBinding) -> ClapProcessPort {
    ClapProcessPort {
        key: binding.mapping.key,
        index: binding.index,
        layout: binding.mapping.layout,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AudioMappingError {
    AllocationFailed,
    IndexNotRepresentable,
    InvalidClapId(PortKey),
    DuplicateMappedPort(PortKey),
    UnknownMappedPort(PortKey),
    InvalidConfiguration(chassis_core::audio::AudioIoConfigurationError),
    DuplicateClapId(u32),
    MultipleMainPorts,
    UnknownInPlacePair { port: PortKey, pair: PortKey },
    InactiveInPlacePair { port: PortKey, pair: PortKey },
    InPlacePairSameDirection { port: PortKey, pair: PortKey },
    InPlacePairChannelMismatch { port: PortKey, pair: PortKey },
    NonReciprocalInPlacePair { port: PortKey, pair: PortKey },
    InPlacePairIndexMismatch { port: PortKey, pair: PortKey },
}

impl fmt::Display for AudioMappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllocationFailed => formatter.write_str("audio mapping allocation failed"),
            Self::IndexNotRepresentable => {
                formatter.write_str("audio port index is not representable on this platform")
            }
            Self::InvalidClapId(port) => {
                write!(
                    formatter,
                    "audio port {} uses the invalid CLAP ID",
                    port.as_str()
                )
            }
            Self::DuplicateMappedPort(port) => {
                write!(
                    formatter,
                    "audio port {} is mapped more than once",
                    port.as_str()
                )
            }
            Self::UnknownMappedPort(port) => {
                write!(
                    formatter,
                    "mapped audio port {} is not declared",
                    port.as_str()
                )
            }
            Self::InvalidConfiguration(error) => {
                write!(formatter, "invalid Chassis audio configuration: {error}")
            }
            Self::DuplicateClapId(id) => {
                write!(
                    formatter,
                    "CLAP audio port ID {id} is duplicated in one direction"
                )
            }
            Self::MultipleMainPorts => {
                formatter.write_str("CLAP allows at most one main port per direction")
            }
            Self::UnknownInPlacePair { port, pair } => write!(
                formatter,
                "audio port {} pairs with undeclared port {}",
                port.as_str(),
                pair.as_str()
            ),
            Self::InactiveInPlacePair { port, pair } => write!(
                formatter,
                "audio port {} pairs with inactive port {}",
                port.as_str(),
                pair.as_str()
            ),
            Self::InPlacePairSameDirection { port, pair } => write!(
                formatter,
                "audio port {} pairs in place with same-direction port {}",
                port.as_str(),
                pair.as_str()
            ),
            Self::InPlacePairChannelMismatch { port, pair } => write!(
                formatter,
                "audio port {} has a different channel count from in-place pair {}",
                port.as_str(),
                pair.as_str()
            ),
            Self::NonReciprocalInPlacePair { port, pair } => write!(
                formatter,
                "audio port {} and in-place pair {} do not reference each other",
                port.as_str(),
                pair.as_str()
            ),
            Self::InPlacePairIndexMismatch { port, pair } => write!(
                formatter,
                "audio port {} cannot align with in-place pair {} at one CLAP process index",
                port.as_str(),
                pair.as_str()
            ),
        }
    }
}

impl std::error::Error for AudioMappingError {}

#[cfg(test)]
mod tests {
    use super::*;
    use chassis_core::audio::{
        AudioPortDescriptor, MAIN_INPUT, MAIN_OUTPUT, PortDirection, PortKey, PortRole,
        SIDECHAIN_INPUT,
    };

    #[test]
    fn default_mapping_projects_main_and_sidechain_without_derived_ids() {
        let configuration = ClapAudioConfiguration::new(
            &chassis_core::audio::DEFAULT_EFFECT_PORTS,
            &DEFAULT_CLAP_AUDIO_PORTS,
        )
        .expect("default mapping is valid");
        assert_eq!(configuration.count(PortDirection::Input), 2);
        assert_eq!(configuration.count(PortDirection::Output), 1);
        let main_input = configuration
            .info(0, PortDirection::Input)
            .expect("main input exists");
        assert_eq!(main_input.id, ClapId::new(0));
        assert!(main_input.flags.contains(AudioPortFlags::IS_MAIN));
        assert_eq!(main_input.in_place_pair, Some(ClapId::new(0)));
        let sidechain = configuration
            .info(1, PortDirection::Input)
            .expect("sidechain exists");
        assert_eq!(sidechain.id, ClapId::new(1));
        assert!(!sidechain.flags.contains(AudioPortFlags::IS_MAIN));
        assert_eq!(sidechain.in_place_pair, None);
        assert_eq!(configuration.audio_io().ports().len(), 3);
        assert_eq!(configuration.audio_io().ports()[0].key, MAIN_INPUT);
        assert_eq!(configuration.audio_io().ports()[1].key, MAIN_OUTPUT);
        assert_eq!(configuration.audio_io().ports()[2].key, SIDECHAIN_INPUT);
        assert!(configuration.process_slots()[0].paired);
        assert_eq!(
            configuration.process_slots()[0]
                .input
                .expect("main slot has an input")
                .index,
            AudioPortIndex::new(0)
        );
        assert_eq!(
            configuration.process_slots()[0]
                .output
                .expect("main slot has an output")
                .index,
            AudioPortIndex::new(1)
        );
        assert_eq!(
            configuration.process_slots()[1]
                .input
                .expect("sidechain slot has an input")
                .key,
            SIDECHAIN_INPUT
        );
        assert_eq!(
            configuration.process_slots()[1]
                .input
                .expect("sidechain slot has an input")
                .index,
            AudioPortIndex::new(2)
        );
        assert!(configuration.process_slots()[1].output.is_none());
    }

    #[test]
    fn f64_support_advertises_common_sample_size() {
        let configuration = ClapAudioConfiguration::new(
            &chassis_core::audio::DEFAULT_EFFECT_PORTS,
            &DEFAULT_CLAP_AUDIO_PORTS,
        )
        .expect("default mapping is valid")
        .with_f64_support();
        let main_input = configuration
            .info(0, PortDirection::Input)
            .expect("main input exists");
        assert!(main_input.flags.contains(AudioPortFlags::SUPPORTS_64BITS));
        assert!(
            main_input
                .flags
                .contains(AudioPortFlags::REQUIRES_COMMON_SAMPLE_SIZE)
        );
        assert!(!main_input.flags.contains(AudioPortFlags::PREFERS_64BITS));
    }

    #[test]
    fn main_port_is_dense_index_zero_even_when_mapping_order_differs() {
        let reordered = [
            DEFAULT_CLAP_AUDIO_PORTS[2],
            DEFAULT_CLAP_AUDIO_PORTS[1],
            DEFAULT_CLAP_AUDIO_PORTS[0],
        ];
        let configuration =
            ClapAudioConfiguration::new(&chassis_core::audio::DEFAULT_EFFECT_PORTS, &reordered)
                .expect("reordered mapping is valid");
        let main = configuration.process_slots()[0];
        assert_eq!(main.input.expect("main input exists").key, MAIN_INPUT);
        assert_eq!(main.input.expect("main input exists").index, AudioPortIndex::new(0));
        assert_eq!(main.output.expect("main output exists").key, MAIN_OUTPUT);
        assert_eq!(main.output.expect("main output exists").index, AudioPortIndex::new(1));
        assert!(main.paired);
        assert_eq!(
            configuration.process_slots()[1]
                .input
                .expect("sidechain exists")
                .key,
            SIDECHAIN_INPUT
        );
    }

    #[test]
    fn duplicate_direction_local_ids_are_rejected() {
        let duplicate = [
            DEFAULT_CLAP_AUDIO_PORTS[0],
            DEFAULT_CLAP_AUDIO_PORTS[1],
            ClapAudioPort::new(SIDECHAIN_INPUT, 0, ChannelLayout::Stereo, None),
        ];
        assert_eq!(
            ClapAudioConfiguration::new(&chassis_core::audio::DEFAULT_EFFECT_PORTS, &duplicate)
                .err(),
            Some(AudioMappingError::DuplicateClapId(0))
        );
    }

    #[test]
    fn missing_required_mapping_is_rejected() {
        let incomplete = [DEFAULT_CLAP_AUDIO_PORTS[0]];
        assert!(matches!(
            ClapAudioConfiguration::new(&chassis_core::audio::DEFAULT_EFFECT_PORTS, &incomplete),
            Err(AudioMappingError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn arbitrary_declared_layouts_can_be_qualified_at_setup_time() {
        const AUX_INPUT: PortKey = PortKey::new("audio.aux.in");
        const PORTS: [AudioPortDescriptor; 3] = [
            AudioPortDescriptor {
                key: MAIN_INPUT,
                name: "Main In",
                direction: PortDirection::Input,
                role: PortRole::Main,
                optional: false,
            },
            AudioPortDescriptor {
                key: AUX_INPUT,
                name: "Aux In",
                direction: PortDirection::Input,
                role: PortRole::Auxiliary,
                optional: true,
            },
            AudioPortDescriptor {
                key: MAIN_OUTPUT,
                name: "Main Out",
                direction: PortDirection::Output,
                role: PortRole::Main,
                optional: false,
            },
        ];
        let mapping = [
            ClapAudioPort::new(MAIN_INPUT, 10, ChannelLayout::Mono, Some(MAIN_OUTPUT)),
            ClapAudioPort::new(MAIN_OUTPUT, 20, ChannelLayout::Mono, Some(MAIN_INPUT)),
            ClapAudioPort::new(AUX_INPUT, 11, ChannelLayout::Mono, None),
        ];
        let configuration = ClapAudioConfiguration::new(&PORTS, &mapping)
            .expect("arbitrary setup mapping is valid");
        assert_eq!(configuration.count(PortDirection::Input), 2);
        assert_eq!(configuration.count(PortDirection::Output), 1);
        assert_eq!(
            configuration
                .info(0, PortDirection::Input)
                .expect("main input exists")
                .channel_count,
            1
        );
        assert_eq!(configuration.process_slots().len(), 2);
        assert!(configuration.process_slots()[0].paired);
        assert_eq!(
            configuration.process_slots()[1]
                .input
                .expect("aux input exists")
                .key,
            AUX_INPUT
        );
        assert_eq!(
            configuration.process_slots()[1]
                .input
                .expect("aux input exists")
                .index,
            AudioPortIndex::new(1)
        );
    }

    #[test]
    fn reciprocal_aux_pairs_align_even_when_mapping_order_differs() {
        const AUX_INPUT: PortKey = PortKey::new("audio.aux.in");
        const AUX_OUTPUT: PortKey = PortKey::new("audio.aux.out");
        const PORTS: [AudioPortDescriptor; 4] = [
            AudioPortDescriptor {
                key: MAIN_INPUT,
                name: "Main In",
                direction: PortDirection::Input,
                role: PortRole::Main,
                optional: false,
            },
            AudioPortDescriptor {
                key: MAIN_OUTPUT,
                name: "Main Out",
                direction: PortDirection::Output,
                role: PortRole::Main,
                optional: false,
            },
            AudioPortDescriptor {
                key: AUX_INPUT,
                name: "Aux In",
                direction: PortDirection::Input,
                role: PortRole::Auxiliary,
                optional: true,
            },
            AudioPortDescriptor {
                key: AUX_OUTPUT,
                name: "Aux Out",
                direction: PortDirection::Output,
                role: PortRole::Auxiliary,
                optional: true,
            },
        ];
        let mapping = [
            ClapAudioPort::new(AUX_OUTPUT, 21, ChannelLayout::Mono, Some(AUX_INPUT)),
            ClapAudioPort::new(MAIN_OUTPUT, 20, ChannelLayout::Stereo, Some(MAIN_INPUT)),
            ClapAudioPort::new(AUX_INPUT, 11, ChannelLayout::Mono, Some(AUX_OUTPUT)),
            ClapAudioPort::new(MAIN_INPUT, 10, ChannelLayout::Stereo, Some(MAIN_OUTPUT)),
        ];
        let configuration =
            ClapAudioConfiguration::new(&PORTS, &mapping).expect("paired aux mapping is valid");
        let slots = configuration.process_slots();
        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].input.expect("main input exists").key, MAIN_INPUT);
        assert_eq!(
            slots[0].output.expect("main output exists").key,
            MAIN_OUTPUT
        );
        assert!(slots[0].paired);
        assert_eq!(slots[1].input.expect("aux input exists").key, AUX_INPUT);
        assert_eq!(slots[1].output.expect("aux output exists").key, AUX_OUTPUT);
        assert!(slots[1].paired);
    }

    #[test]
    fn unrelated_dense_ports_remain_semantically_independent() {
        const AUX_INPUT: PortKey = PortKey::new("audio.aux.in");
        const AUX_OUTPUT: PortKey = PortKey::new("audio.aux.out");
        const PORTS: [AudioPortDescriptor; 4] = [
            AudioPortDescriptor {
                key: MAIN_INPUT,
                name: "Main In",
                direction: PortDirection::Input,
                role: PortRole::Main,
                optional: false,
            },
            AudioPortDescriptor {
                key: MAIN_OUTPUT,
                name: "Main Out",
                direction: PortDirection::Output,
                role: PortRole::Main,
                optional: false,
            },
            AudioPortDescriptor {
                key: AUX_INPUT,
                name: "Aux In",
                direction: PortDirection::Input,
                role: PortRole::Auxiliary,
                optional: true,
            },
            AudioPortDescriptor {
                key: AUX_OUTPUT,
                name: "Aux Out",
                direction: PortDirection::Output,
                role: PortRole::Auxiliary,
                optional: true,
            },
        ];
        let mapping = [
            ClapAudioPort::new(MAIN_INPUT, 10, ChannelLayout::Stereo, Some(MAIN_OUTPUT)),
            ClapAudioPort::new(AUX_OUTPUT, 21, ChannelLayout::Stereo, None),
            ClapAudioPort::new(MAIN_OUTPUT, 20, ChannelLayout::Stereo, Some(MAIN_INPUT)),
            ClapAudioPort::new(AUX_INPUT, 11, ChannelLayout::Mono, None),
        ];
        let configuration = ClapAudioConfiguration::new(&PORTS, &mapping)
            .expect("independent aux mapping is valid");
        let slot = configuration.process_slots()[1];
        assert_eq!(slot.input.expect("aux input exists").key, AUX_INPUT);
        assert_eq!(slot.output.expect("aux output exists").key, AUX_OUTPUT);
        assert!(!slot.paired);
    }
}
