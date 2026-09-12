//! In-process CLAP proof for plugin -> host -> plugin main-thread re-entrancy.

use std::{
    cell::OnceCell,
    convert::Infallible,
    io::Cursor,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use chassis_clap::{ClapStereoEffect, SingleComponentEntry};
use chassis_core::{
    parameters::ParameterDescriptor,
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource},
    runtime::{Component, LatencySamples, Process, Processor},
};
use clack_extensions::{
    latency::{HostLatency, HostLatencyImpl, PluginLatency},
    params::{
        HostParams, HostParamsImplMainThread, HostParamsImplShared, ParamClearFlags,
        ParamRescanFlags, PluginParams,
    },
    state::PluginState,
};
use clack_host::{factory::plugin::PluginFactory, prelude::*};

const VALUE_CLAP_ID: u32 = 29;

struct ReentrantProbe {
    parameters: Vec<ParameterDescriptor>,
}

impl Default for ReentrantProbe {
    fn default() -> Self {
        Self {
            parameters: vec![
                ParameterDescriptor::float("value", "Value", 0.0, 1.0, 0.5)
                    .expect("probe parameter is valid"),
            ],
        }
    }
}

impl Component for ReentrantProbe {
    type Processor = ReentrantProcessor;
    type ActivationError = Infallible;

    fn schema(
        &self,
    ) -> Result<chassis_core::schema::ComponentSchema, chassis_core::schema::ComponentSchemaError>
    {
        chassis_core::schema::ComponentSchema::stereo_effect(self.parameters.clone())
    }

    fn activate(
        &self,
        config: &ActivationConfig<'_>,
    ) -> Result<Self::Processor, Self::ActivationError> {
        let latency = if config.process().sample_rate() >= 88_200.0 {
            64
        } else {
            32
        };
        Ok(ReentrantProcessor(LatencySamples::new(latency)))
    }
}

struct ReentrantProcessor(LatencySamples);

impl Processor for ReentrantProcessor {
    fn latency(&self) -> LatencySamples {
        self.0
    }
}

impl Process<f32> for ReentrantProcessor {
    fn process<B>(&mut self, _block: &mut ProcessBlock<'_, '_, '_, f32, B>)
    where
        B: ProcessBufferSource<f32> + ?Sized,
    {
    }
}

impl ClapStereoEffect for ReentrantProbe {
    const CLAP_ID: &'static str = "org.nijaru.chassis.reentrant-probe";
    const CLAP_NAME: &'static str = "Chassis Reentrant Probe";
    const CLAP_PARAMETER_IDS: &'static [(&'static str, u32)] = &[("value", VALUE_CLAP_ID)];
    const CLAP_MAX_PARAMETER_EVENTS: u32 = 1;
}

struct TestHostShared {
    param_queries: Arc<AtomicU32>,
    latency_queries: Arc<AtomicU32>,
}

struct TestHostMainThread<'a> {
    instance: OnceCell<InitializedPluginHandle<'a>>,
    param_queries: Arc<AtomicU32>,
    latency_queries: Arc<AtomicU32>,
}

struct TestHostAudioProcessor;
struct TestHostHandlers;

impl SharedHandler<'_> for TestHostShared {
    fn request_restart(&self) {}
    fn request_process(&self) {}
    fn request_callback(&self) {}
}

impl HostParamsImplShared for TestHostShared {
    fn request_flush(&self) {}
}

impl<'a> MainThreadHandler<'a> for TestHostMainThread<'a> {
    fn initialized(&self, instance: InitializedPluginHandle<'a>) {
        assert!(self.instance.set(instance).is_ok());
    }
}

impl HostParamsImplMainThread for TestHostMainThread<'_> {
    fn rescan(&self, flags: ParamRescanFlags) {
        assert!(flags.contains(ParamRescanFlags::VALUES));
        let instance = self.instance.get().expect("plugin is initialized");
        assert!(instance.get_extension::<PluginParams>().is_some());
        self.param_queries.fetch_add(1, Ordering::Relaxed);
    }

    fn clear(&self, _param_id: ClapId, _flags: ParamClearFlags) {}
}

impl HostLatencyImpl for TestHostMainThread<'_> {
    fn changed(&self) {
        let instance = self.instance.get().expect("plugin is initialized");
        assert!(instance.get_extension::<PluginLatency>().is_some());
        self.latency_queries.fetch_add(1, Ordering::Relaxed);
    }
}

impl HostHandlers for TestHostHandlers {
    type Shared<'a> = TestHostShared;
    type MainThread<'a> = TestHostMainThread<'a>;
    type AudioProcessor<'a> = TestHostAudioProcessor;

    fn declare_extensions(builder: &mut HostExtensions<Self>, _shared: &Self::Shared<'_>) {
        builder.register::<HostParams>().register::<HostLatency>();
    }
}

impl AudioProcessorHandler<'_> for TestHostAudioProcessor {}

#[test]
fn state_rescan_and_latency_change_allow_reentrant_plugin_queries() {
    let entry = PluginEntry::load_from_clack::<SingleComponentEntry<ReentrantProbe>>(c"")
        .expect("static reentrant probe entry loads");
    let descriptor = entry
        .get_factory::<PluginFactory>()
        .expect("plugin factory exists")
        .plugin_descriptor(0)
        .expect("reentrant probe descriptor exists");
    let host_info = HostInfo::new("chassis-test", "", "", "").expect("host info is valid");
    let param_queries = Arc::new(AtomicU32::new(0));
    let latency_queries = Arc::new(AtomicU32::new(0));
    let shared_param_queries = Arc::clone(&param_queries);
    let shared_latency_queries = Arc::clone(&latency_queries);
    let mut plugin = PluginInstance::<TestHostHandlers>::new(
        move |()| TestHostShared {
            param_queries: shared_param_queries,
            latency_queries: shared_latency_queries,
        },
        |shared| TestHostMainThread {
            instance: OnceCell::new(),
            param_queries: Arc::clone(&shared.param_queries),
            latency_queries: Arc::clone(&shared.latency_queries),
        },
        &entry,
        descriptor.id().expect("reentrant probe id is valid"),
        &host_info,
    )
    .expect("reentrant probe instantiates");

    let plugin_handle = plugin.plugin_handle();
    let state = plugin_handle
        .get_extension::<PluginState>()
        .expect("reentrant probe exports state");
    let mut encoded = Vec::new();
    state
        .save(&plugin_handle, &mut encoded)
        .expect("probe state saves");
    state
        .load(&plugin_handle, &mut Cursor::new(encoded))
        .expect("probe state reloads while host re-enters parameter discovery");
    assert_eq!(param_queries.load(Ordering::Relaxed), 1);

    let processor = plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 48_000.0,
                min_frames_count: 1,
                max_frames_count: 64,
            },
        )
        .expect("first activation survives reentrant latency discovery");
    assert_eq!(latency_queries.load(Ordering::Relaxed), 1);
    plugin.deactivate(processor);

    let processor = plugin
        .activate(
            |_, _| TestHostAudioProcessor,
            PluginAudioConfiguration {
                sample_rate: 96_000.0,
                min_frames_count: 1,
                max_frames_count: 128,
            },
        )
        .expect("reactivation survives a second reentrant latency discovery");
    assert_eq!(latency_queries.load(Ordering::Relaxed), 2);
    plugin.deactivate(processor);
}
