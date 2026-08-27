//! Explicit component/processor lifecycle contracts.
//!
//! This module is intentionally small. It proves the semantic ownership split
//! before Chassis adds parameter/state machinery, proc macros, or format adapters.

use core::fmt;

use crate::{
    audio::{
        AudioIoConfiguration, AudioIoConfigurationError, AudioPortDescriptor,
        DEFAULT_EFFECT_PORTS,
    },
    buffer::ChannelBuffer,
    process::{ActivationConfig, ProcessBlock, ProcessBlockError, ProcessConfig, ProcessMode},
};

/// Immutable product definition/factory for one Chassis component type.
///
/// The component is not the live mutable processor. An ordinary effect can use
/// the default stereo-main/optional-sidechain port schema and only implement
/// activation. Instruments and unusual processors override [`Self::audio_ports`]
/// without changing the processor lifecycle model.
pub trait Component {
    /// Realtime processor created for one successful activation.
    type Processor: Processor;

    /// Product-specific activation failure.
    type ActivationError;

    /// Return the stable audio-port schema for this component.
    ///
    /// The explicit pre-alpha API defaults to the standard effect convention.
    /// Components with different I/O override this method.
    #[must_use]
    fn audio_ports(&self) -> &[AudioPortDescriptor] {
        &DEFAULT_EFFECT_PORTS
    }

    /// Create the exclusively-owned realtime processor for one activation.
    ///
    /// All allocation/precomputation required by the processor belongs here or
    /// in product helpers called from here, before realtime processing begins.
    ///
    /// # Errors
    ///
    /// Returns the product-defined activation error when required resources or
    /// product invariants cannot be established.
    fn activate(
        &self,
        config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError>;
}

/// Mutable realtime DSP/runtime-history owner while a component is active.
///
/// The format-independent core does not require [`Send`] because a Chassis
/// component may be embedded in a deliberately same-thread runtime. A deployment
/// adapter that transfers processor ownership across threads must add the
/// appropriate `Send` bound at that boundary. `Sync` is not a normal processing
/// requirement because processor mutation remains exclusive.
///
/// `Processor` deliberately contains lifecycle/reset semantics only. Sample
/// precision is expressed by implementing [`Process`], so adding an f64 path
/// later does not require a second processor architecture.
pub trait Processor {
    /// Reset transient DSP history while preserving persistent/control state.
    ///
    /// Stateless processors may use the default no-op implementation.
    fn reset(&mut self) {}
}

/// Processing capability for one sample representation.
pub trait Process<S>: Processor {
    /// Process one already-validated realtime block.
    fn process(&mut self, block: &mut ProcessBlock<'_, '_, S>);
}

/// Framework activation failure before an active processor is published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivateError<E> {
    /// The proposed whole-component I/O configuration failed structural validation.
    InvalidAudioIo(AudioIoConfigurationError),
    /// Product activation failed after the framework configuration was validated.
    Product(E),
}

impl<E> fmt::Display for ActivateError<E>
where
    E: fmt::Display,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAudioIo(error) => write!(formatter, "invalid audio I/O: {error}"),
            Self::Product(error) => write!(formatter, "product activation failed: {error}"),
        }
    }
}

impl<E> std::error::Error for ActivateError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidAudioIo(error) => Some(error),
            Self::Product(error) => Some(error),
        }
    }
}

/// Active processor plus the immutable configuration that bounds its realtime work.
///
/// This is the first executable lifecycle shell, not the eventual complete
/// `InstanceRuntime`: canonical parameter/state authority will be added only
/// when that storage/publication contract is implemented.
pub struct Activated<'a, P> {
    config: ActivationConfig<'a>,
    processor: P,
}

impl<'a, P> Activated<'a, P>
where
    P: Processor,
{
    /// Return the immutable activation configuration.
    #[must_use]
    pub const fn config(&self) -> ActivationConfig<'a> {
        self.config
    }

    /// Reset transient realtime processor history.
    pub fn reset(&mut self) {
        self.processor.reset();
    }

    /// Process one block after validating callback-varying dimensions.
    ///
    /// Stable endpoint/schema translation is intentionally not rescanned here;
    /// adapters resolve that mapping outside the realtime hot path.
    ///
    /// # Errors
    ///
    /// Returns [`ProcessBlockError`] before product DSP runs if callback frame
    /// dimensions violate the activation contract.
    pub fn process<S>(
        &mut self,
        frame_count: u32,
        mode: ProcessMode,
        buffers: &mut [ChannelBuffer<'_, S>],
    ) -> Result<(), ProcessBlockError>
    where
        P: Process<S>,
    {
        let mut block = ProcessBlock::new(&self.config, frame_count, mode, buffers)?;
        self.processor.process(&mut block);
        Ok(())
    }

    /// Consume the active lifecycle shell and destroy its processor in the caller's domain.
    ///
    /// Adapters must call this only after their backend contract guarantees that
    /// no process/reset borrow remains in flight. Future runtimes with deferred
    /// resources/tasks may replace this simple destruction path.
    pub fn deactivate(self) {
        let Self { processor, .. } = self;
        drop(processor);
    }
}

/// Validate a component's structural I/O schema and create an active processor.
///
/// Product-specific whole-I/O policy is not yet modeled beyond the structural
/// schema rules in [`AudioIoConfiguration::validate`]. That remains an explicit
/// pre-release freeze gate rather than an implicit boolean convention.
///
/// # Errors
///
/// Returns [`ActivateError::InvalidAudioIo`] before calling product activation
/// for malformed configurations, or [`ActivateError::Product`] when the product
/// rejects activation.
pub fn activate<'a, C>(
    component: &C,
    process: ProcessConfig,
    audio_io: AudioIoConfiguration<'a>,
) -> Result<Activated<'a, C::Processor>, ActivateError<C::ActivationError>>
where
    C: Component,
{
    audio_io
        .validate(component.audio_ports())
        .map_err(ActivateError::InvalidAudioIo)?;

    let config = ActivationConfig::new(process, audio_io);
    let processor = component.activate(&config).map_err(ActivateError::Product)?;

    Ok(Activated { config, processor })
}
