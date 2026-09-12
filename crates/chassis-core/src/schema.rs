//! Coherent immutable component schema and semantic state identity.
//!
//! This module is the migration target for the current incremental component
//! metadata APIs. It deliberately owns setup/control-domain metadata while
//! process-time code uses dense indices derived from a validated schema.

use core::fmt;
use std::string::String;
use std::vec::Vec;

use crate::{
    audio::{
        AudioIoConfigurationError, AudioPortDescriptor, AudioPortIndex, DEFAULT_EFFECT_PORTS,
        PortKey, audio_port_index, validate_audio_port_schema,
    },
    events::{
        EventPortDescriptor, EventPortIndex, EventPortKey, EventPortSchemaError, event_port_index,
        validate_event_port_schema,
    },
    parameters::{ParameterDescriptor, ParameterStore, ParameterStoreError},
};

/// Stable semantic identity for one Chassis component's persistent state.
///
/// Deployment identities such as CLAP IDs, VST3 class IDs, AU subtype codes, or
/// application bundle IDs are separate projections and must not become this
/// identity accidentally.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentId(String);

impl ComponentId {
    /// Construct a stable semantic component identity.
    ///
    /// IDs are non-empty and may not contain whitespace or control characters.
    /// Reverse-domain strings are recommended for independently distributed
    /// components but are not required by core.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentIdError`] when the identity is empty or contains
    /// whitespace/control characters.
    pub fn new(value: impl Into<String>) -> Result<Self, ComponentIdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ComponentIdError::Empty);
        }
        if let Some(character) = value.chars().find(|character| character.is_whitespace()) {
            return Err(ComponentIdError::Whitespace(character));
        }
        if let Some(character) = value.chars().find(|character| character.is_control()) {
            return Err(ComponentIdError::Control(character));
        }
        Ok(Self(value))
    }

    /// Return the canonical identity text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consume the identity and return its text.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl AsRef<str> for ComponentId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ComponentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Invalid semantic component identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentIdError {
    /// Identity was empty.
    Empty,
    /// Identity contained whitespace.
    Whitespace(char),
    /// Identity contained a control character.
    Control(char),
}

impl fmt::Display for ComponentIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("component identity is empty"),
            Self::Whitespace(character) => write!(
                formatter,
                "component identity contains whitespace {character:?}"
            ),
            Self::Control(character) => write!(
                formatter,
                "component identity contains control character {character:?}"
            ),
        }
    }
}

impl std::error::Error for ComponentIdError {}

/// Version of the component's semantic persistent-state schema.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateSchemaVersion(u32);

impl StateSchemaVersion {
    /// Construct a state-schema version.
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the numeric schema version.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Stable semantic state identity owned by one component schema.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StateIdentity {
    component: ComponentId,
    schema: StateSchemaVersion,
}

impl StateIdentity {
    /// Construct one semantic state identity/version pair.
    #[must_use]
    pub const fn new(component: ComponentId, schema: StateSchemaVersion) -> Self {
        Self { component, schema }
    }

    /// Return the stable semantic component identity.
    #[must_use]
    pub const fn component(&self) -> &ComponentId {
        &self.component
    }

    /// Return the semantic state-schema version.
    #[must_use]
    pub const fn schema(&self) -> StateSchemaVersion {
        self.schema
    }
}

/// One coherent immutable semantic schema for a component instance generation.
///
/// The schema owns setup-domain metadata so dynamically constructed components
/// are not forced to rely on Rust declaration order as compatibility identity.
/// The current audio/event descriptor types still contain static strings; those
/// identities will migrate to owned validated keys before this surface freezes.
///
/// `state_identity` is temporarily optional while the existing adapters/runtime
/// state APIs are migrated from caller-supplied identity. New identified schemas
/// should use [`Self::new`]. [`Self::unidentified`] exists only as a pre-alpha
/// migration bridge and is not the target stable authoring surface.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentSchema {
    state_identity: Option<StateIdentity>,
    audio_ports: Vec<AudioPortDescriptor>,
    event_ports: Vec<EventPortDescriptor>,
    parameters: Vec<ParameterDescriptor>,
}

impl ComponentSchema {
    /// Construct and validate one complete identified immutable schema.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentSchemaError`] when the audio, event, or parameter
    /// schema is invalid.
    pub fn new(
        id: ComponentId,
        state_schema: StateSchemaVersion,
        audio_ports: Vec<AudioPortDescriptor>,
        event_ports: Vec<EventPortDescriptor>,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::build(
            Some(StateIdentity::new(id, state_schema)),
            audio_ports,
            event_ports,
            parameters,
        )
    }

    /// Construct a validated schema without semantic state identity.
    ///
    /// This is a migration bridge for current pre-alpha components whose state
    /// identity is still supplied by deployment adapters/callers. Do not use it
    /// as the basis of a stable public API.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentSchemaError`] when the audio, event, or parameter
    /// schema is invalid.
    pub fn unidentified(
        audio_ports: Vec<AudioPortDescriptor>,
        event_ports: Vec<EventPortDescriptor>,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::build(None, audio_ports, event_ports, parameters)
    }

    /// Construct the conventional stereo-effect schema with optional sidechain.
    ///
    /// This is an authoring convenience above neutral core semantics; components
    /// with different audio/event I/O construct their schema explicitly.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentSchemaError`] when the supplied parameter schema is invalid.
    pub fn stereo_effect(
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::unidentified(DEFAULT_EFFECT_PORTS.to_vec(), Vec::new(), parameters)
    }

    /// Construct a conventional stereo-effect schema with semantic state identity.
    ///
    /// # Errors
    ///
    /// Returns [`ComponentSchemaError`] when the supplied parameter schema is invalid.
    pub fn stereo_effect_with_state(
        id: ComponentId,
        state_schema: StateSchemaVersion,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        Self::new(
            id,
            state_schema,
            DEFAULT_EFFECT_PORTS.to_vec(),
            Vec::new(),
            parameters,
        )
    }

    fn build(
        state_identity: Option<StateIdentity>,
        audio_ports: Vec<AudioPortDescriptor>,
        event_ports: Vec<EventPortDescriptor>,
        parameters: Vec<ParameterDescriptor>,
    ) -> Result<Self, ComponentSchemaError> {
        validate_audio_port_schema(&audio_ports).map_err(ComponentSchemaError::AudioPorts)?;
        validate_event_port_schema(&event_ports).map_err(ComponentSchemaError::EventPorts)?;
        ParameterStore::new(&parameters).map_err(ComponentSchemaError::Parameters)?;

        Ok(Self {
            state_identity,
            audio_ports,
            event_ports,
            parameters,
        })
    }

    /// Return semantic state identity when this schema has been fully migrated.
    #[must_use]
    pub const fn state_identity(&self) -> Option<&StateIdentity> {
        self.state_identity.as_ref()
    }

    /// Return the immutable audio-port schema.
    #[must_use]
    pub fn audio_ports(&self) -> &[AudioPortDescriptor] {
        &self.audio_ports
    }

    /// Return the immutable event-port schema.
    #[must_use]
    pub fn event_ports(&self) -> &[EventPortDescriptor] {
        &self.event_ports
    }

    /// Return the immutable parameter schema.
    #[must_use]
    pub fn parameters(&self) -> &[ParameterDescriptor] {
        &self.parameters
    }

    /// Resolve a stable audio-port key to its dense schema-local index.
    #[must_use]
    pub fn audio_port_index(&self, key: PortKey) -> Option<AudioPortIndex> {
        audio_port_index(&self.audio_ports, key)
    }

    /// Resolve a stable event-port key to its dense schema-local index.
    #[must_use]
    pub fn event_port_index(&self, key: EventPortKey) -> Option<EventPortIndex> {
        event_port_index(&self.event_ports, key)
    }
}

/// Invalid complete component schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentSchemaError {
    /// Audio-port schema is invalid.
    AudioPorts(AudioIoConfigurationError),
    /// Event-port schema is invalid.
    EventPorts(EventPortSchemaError),
    /// Parameter schema is invalid.
    Parameters(ParameterStoreError),
}

impl fmt::Display for ComponentSchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AudioPorts(error) => write!(formatter, "invalid audio ports: {error}"),
            Self::EventPorts(error) => write!(formatter, "invalid event ports: {error}"),
            Self::Parameters(error) => write!(formatter, "invalid parameters: {error}"),
        }
    }
}

impl std::error::Error for ComponentSchemaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::AudioPorts(error) => Some(error),
            Self::EventPorts(error) => Some(error),
            Self::Parameters(error) => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioPortIndex, DEFAULT_EFFECT_PORTS, MAIN_INPUT};
    use crate::events::{EventDialect, EventPortDirection};
    use crate::parameters::ParameterDescriptor;

    const NOTE_DIALECTS: &[EventDialect] = &[EventDialect::Notes];

    #[test]
    fn coherent_schema_resolves_dense_port_indices() {
        let note_key = EventPortKey::new("notes.in");
        let schema = ComponentSchema::new(
            ComponentId::new("org.nijaru.schema-probe").expect("component id is valid"),
            StateSchemaVersion::new(1),
            DEFAULT_EFFECT_PORTS.to_vec(),
            vec![EventPortDescriptor {
                key: note_key,
                name: "Notes",
                direction: EventPortDirection::Input,
                dialects: NOTE_DIALECTS,
            }],
            vec![
                ParameterDescriptor::float("gain", "Gain", 0.0, 2.0, 1.0)
                    .expect("parameter is valid"),
            ],
        )
        .expect("component schema is valid");

        let identity = schema.state_identity().expect("identified schema");
        assert_eq!(identity.component().as_str(), "org.nijaru.schema-probe");
        assert_eq!(identity.schema(), StateSchemaVersion::new(1));
        assert_eq!(
            schema.audio_port_index(MAIN_INPUT),
            Some(AudioPortIndex::new(0))
        );
        assert_eq!(
            schema.event_port_index(note_key),
            Some(EventPortIndex::new(0))
        );
        assert_eq!(schema.parameters().len(), 1);
    }

    #[test]
    fn unidentified_schema_is_explicitly_identity_free() {
        let schema = ComponentSchema::unidentified(DEFAULT_EFFECT_PORTS.to_vec(), vec![], vec![])
            .expect("migration schema is valid");
        assert_eq!(schema.state_identity(), None);
    }

    #[test]
    fn component_identity_rejects_unstable_text() {
        assert_eq!(ComponentId::new(""), Err(ComponentIdError::Empty));
        assert_eq!(
            ComponentId::new("bad id"),
            Err(ComponentIdError::Whitespace(' '))
        );
    }
}
