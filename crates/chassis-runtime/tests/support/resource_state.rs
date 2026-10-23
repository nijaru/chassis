//! Resource-backed complete-state client shared by semantic and realtime probes.

use chassis_runtime::{
    automation::ParameterEventValue,
    parameters::{ChoiceOption, ParameterDescriptor, ParameterIndex, ParameterStore},
    process::{ActivationConfig, ProcessBlock, ProcessBufferSource, ProcessChannel},
    runtime::{Component, ComponentStateError, Process, Processor},
    schema::{ComponentId, ComponentSchema, ComponentSchemaError, StateSchemaVersion},
    state::{StateDocument, StateEntry, StateMigration, StateValue},
};
use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

pub struct TableComponent {
    pub defaults: Vec<StateEntry>,
    pub identified: bool,
    pub incompatible_controls: Arc<AtomicUsize>,
}
impl Default for TableComponent {
    fn default() -> Self {
        Self {
            defaults: vec![StateEntry::new("state/table", StateValue::Bytes(vec![2]))],
            identified: true,
            incompatible_controls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

fn table(custom: &[StateEntry]) -> Option<&[u8]> {
    let [entry] = custom else {
        return None;
    };
    if entry.key() != "state/table" {
        return None;
    }
    match entry.value() {
        StateValue::Bytes(bytes) if (1..=2).contains(&bytes.len()) => Some(bytes),
        _ => None,
    }
}

#[derive(Debug)]
struct InvalidTable;
impl std::fmt::Display for InvalidTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("index must address the complete table")
    }
}
impl std::error::Error for InvalidTable {}

struct RenameIndex;
impl StateMigration for RenameIndex {
    type Error = ComponentStateError;
    fn source_schema(&self) -> u32 {
        1
    }
    fn to_schema(&self) -> u32 {
        2
    }
    fn migrate(&self, doc: StateDocument) -> Result<StateDocument, Self::Error> {
        // A product migration may rely on the runtime's identity admission.
        assert_eq!(doc.product_id(), "test.resource-table");
        let mut result =
            StateDocument::new(doc.product_id(), 2).map_err(ComponentStateError::new)?;
        for entry in doc.entries() {
            let key = if entry.key() == "parameter/old_index" {
                "parameter/index"
            } else {
                entry.key()
            };
            result
                .insert(StateEntry::new(key, entry.value().clone()))
                .map_err(ComponentStateError::new)?;
        }
        Ok(result)
    }
}

impl Component for TableComponent {
    type Processor = TableProcessor;
    type ActivationError = Infallible;
    fn schema(&self) -> Result<ComponentSchema, ComponentSchemaError> {
        let params = vec![
            ParameterDescriptor::float("index", "Index", 0.0, 1.0, 0.0).unwrap(),
            ParameterDescriptor::choice(
                "mode",
                "Mode",
                vec![
                    ChoiceOption::new("clean", "Clean").unwrap(),
                    ChoiceOption::new("warm", "Warm").unwrap(),
                ],
                "clean",
            )
            .unwrap(),
        ];
        if self.identified {
            ComponentSchema::stereo_effect_with_state(
                ComponentId::new("test.resource-table").unwrap(),
                StateSchemaVersion::new(2),
                params,
            )
        } else {
            ComponentSchema::stereo_effect(params)
        }
    }
    fn default_custom_state(&self) -> Vec<StateEntry> {
        self.defaults.clone()
    }
    fn validate_state(
        &self,
        params: &ParameterStore,
        custom: &[StateEntry],
    ) -> Result<(), ComponentStateError> {
        let Some(table) = table(custom) else {
            return Err(ComponentStateError::new(InvalidTable));
        };
        let Some(ParameterEventValue::Float(index)) =
            params.get_event_value(ParameterIndex::new(0))
        else {
            unreachable!("validated float");
        };
        if usize::from(index >= 0.5) >= table.len() {
            return Err(ComponentStateError::new(InvalidTable));
        }
        Ok(())
    }
    fn state_migrations(&self) -> &[&dyn StateMigration<Error = ComponentStateError>] {
        &[&RenameIndex]
    }
    fn activate(
        &self,
        _: &ActivationConfig<'_>,
        _: &ParameterStore,
        custom: &[StateEntry],
    ) -> Result<TableProcessor, Infallible> {
        Ok(TableProcessor {
            table: table(custom).unwrap().to_vec(),
            incompatible: Arc::clone(&self.incompatible_controls),
            restart: false,
        })
    }
}

pub struct TableProcessor {
    table: Vec<u8>,
    incompatible: Arc<AtomicUsize>,
    restart: bool,
}
impl Processor for TableProcessor {
    fn restart_requested(&self) -> bool {
        self.restart
    }
}
impl Process<f64> for TableProcessor {
    fn process<B: ProcessBufferSource<f64> + ?Sized>(
        &mut self,
        block: &mut ProcessBlock<'_, '_, '_, f64, B>,
    ) {
        let Some(ParameterEventValue::Float(initial)) =
            block.parameters().get_event_value(ParameterIndex::new(0))
        else {
            unreachable!("float schema");
        };
        let events = block.parameter_events();
        for mut channel in block.channels() {
            let mut cursor = events
                .float_cursor(ParameterIndex::new(0), initial)
                .unwrap();
            for (offset, output) in channel.output_mut().unwrap().iter_mut().enumerate() {
                let index =
                    usize::from(cursor.value_at(u32::try_from(offset).unwrap()).unwrap() >= 0.5);
                if index >= self.table.len() {
                    // An ordinary incompatible parameter edit requests restart,
                    // but processing remains valid with the old table meanwhile.
                    self.incompatible.fetch_add(1, Ordering::Relaxed);
                    self.restart = true;
                    *output = f64::from(self.table[0]);
                } else {
                    *output = f64::from(self.table[index]);
                }
            }
        }
    }
}

pub fn document(index: f64, bytes: &[u8]) -> StateDocument {
    let mut doc = StateDocument::new("test.resource-table", 2).unwrap();
    for entry in [
        StateEntry::new("parameter/index", StateValue::Float(index)),
        StateEntry::new("parameter/mode", StateValue::Choice("warm".into())),
        StateEntry::new("state/table", StateValue::Bytes(bytes.to_vec())),
    ] {
        doc.insert(entry).unwrap();
    }
    doc
}
