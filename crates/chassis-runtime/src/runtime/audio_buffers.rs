//! Activation-owned scratch for whole callback topology validation.

use core::ops::ControlFlow;

use crate::{
    audio::{AudioEndpointError, AudioPortIndex, PortDirection, ResolvedAudioIoConfiguration},
    process::ProcessBufferSource,
};

pub(super) struct AudioBufferValidation {
    offsets: Vec<Option<usize>>,
    seen: Vec<bool>,
}

impl AudioBufferValidation {
    pub(super) fn new(resolved: &ResolvedAudioIoConfiguration) -> Result<Self, ()> {
        let mut offsets = Vec::new();
        offsets
            .try_reserve_exact(resolved.schema_port_count())
            .map_err(|_| ())?;
        offsets.resize(resolved.schema_port_count(), None);
        let mut count = 0_usize;
        for port in resolved.active_ports() {
            let index = usize::try_from(port.index().get()).map_err(|_| ())?;
            offsets[index] = Some(count);
            let channels = usize::try_from(port.layout().channel_count()).map_err(|_| ())?;
            count = count.checked_add(channels).ok_or(())?;
        }
        let mut seen = Vec::new();
        seen.try_reserve_exact(count).map_err(|_| ())?;
        seen.resize(count, false);
        Ok(Self { offsets, seen })
    }

    pub(super) fn validate<S>(
        &mut self,
        source: &mut dyn ProcessBufferSource<S>,
        resolved: &ResolvedAudioIoConfiguration,
    ) -> Result<(), AudioEndpointError> {
        self.seen.fill(false);
        let mut count = 0;
        let mut result = Ok(());
        let _ = source.visit_channels(&mut |channel| {
            result = (|| {
                let input = channel.input_endpoint();
                let output = channel.output_endpoint();
                if input.is_none() && output.is_none() {
                    return Err(AudioEndpointError::EmptyChannel);
                }
                if let Some(endpoint) = input {
                    self.visit(
                        resolved,
                        endpoint.port_index(),
                        endpoint.channel(),
                        PortDirection::Input,
                    )?;
                    count += 1;
                }
                if let Some(endpoint) = output {
                    self.visit(
                        resolved,
                        endpoint.port_index(),
                        endpoint.channel(),
                        PortDirection::Output,
                    )?;
                    count += 1;
                }
                Ok(())
            })();
            // Every view consumes a unique endpoint, or stops traversal now.
            if result.is_err() {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        result?;
        if count != self.seen.len() {
            return Err(AudioEndpointError::IncompleteTopology {
                expected: self.seen.len(),
                actual: count,
            });
        }
        Ok(())
    }

    fn visit(
        &mut self,
        resolved: &ResolvedAudioIoConfiguration,
        port: AudioPortIndex,
        channel: u32,
        direction: PortDirection,
    ) -> Result<(), AudioEndpointError> {
        resolved.validate_endpoint(port, channel, direction)?;
        // Endpoint validation proves the port is active and the channel is in
        // range; setup proved both indices and their sum fit the scratch table.
        let offset = self.offsets[usize::try_from(port.get()).expect("validated port index fits")]
            .expect("validated port is active");
        let index = offset + usize::try_from(channel).expect("validated channel index fits");
        if self.seen[index] {
            return Err(AudioEndpointError::DuplicateEndpoint { port, channel });
        }
        self.seen[index] = true;
        Ok(())
    }
}
