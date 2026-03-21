// src/memory.rs
use std::collections::HashMap;
use thiserror::Error;

#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("Value consumed: attempted to read a destructively read value")]
    AlreadyConsumed,
    #[error("Structural decay: attempted to move or send a decayed parent")]
    StructurallyDecayed,
    #[error("Type mismatch: attempted structural access on a non-struct payload")]
    NotAStruct,
    #[error("Memory budget exceeded: {0} bytes required, but only {1} available")]
    OutOfMemory(u64, u64),
    #[error("Clone budget exceeded")]
    CloneBudgetExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntropicState {
    Valid(Payload),
    Decayed(HashMap<String, EntropicState>),
    Consumed,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    Integer(i64),
    String(String),
    Struct(HashMap<String, EntropicState>),
    Array(Vec<Payload>),
}

impl Payload {
    /// Deterministic size calculation for ICTL payloads
    pub fn weight(&self) -> u64 {
        match self {
            Payload::Integer(_) => 8,
            Payload::String(s) => s.len() as u64,
            Payload::Struct(fields) => {
                let fields_weight: u64 = fields
                    .iter()
                    .map(|(_, state)| match state {
                        EntropicState::Valid(p) => p.weight(),
                        EntropicState::Decayed(f) => f
                            .values()
                            .map(|s| match s {
                                EntropicState::Valid(p) => p.weight(),
                                _ => 0,
                            })
                            .sum(),
                        _ => 0,
                    })
                    .sum();
                // 16 bytes overhead for struct metadata/map pointers
                fields_weight + 16
            }
            Payload::Array(elems) => {
                let total: u64 = elems.iter().map(|p| p.weight()).sum();
                total + 16
            }
        }
    }
}

#[derive(Clone)]
pub struct Arena {
    pub capacity: u64,
    pub used: u64,
    pub(crate) bindings: HashMap<String, EntropicState>,
}

impl Arena {
    pub fn new(capacity: u64) -> Self {
        Self {
            capacity,
            used: 0,
            bindings: HashMap::new(),
        }
    }

    /// Checks and reserves memory before insertion
    pub fn insert(
        &mut self,
        identifier: String,
        state: EntropicState,
    ) -> Result<(), MemoryError> {
        if let EntropicState::Valid(ref p) = state {
            let weight = p.weight();
            if self.used + weight > self.capacity {
                return Err(MemoryError::OutOfMemory(
                    weight,
                    self.capacity - self.used,
                ));
            }
            self.used += weight;
        }
        self.bindings.insert(identifier, state);
        Ok(())
    }

    /// Destructive read of a whole identifier.
    /// Rejects moves of Decayed values to preserve structural integrity.
    pub fn consume(&mut self, identifier: &str) -> Result<Payload, MemoryError> {
        match self.bindings.remove(identifier) {
            Some(EntropicState::Valid(payload)) => {
                // Free memory on total consumption
                self.used -= payload.weight();
                self.bindings
                    .insert(identifier.to_string(), EntropicState::Consumed);
                Ok(payload)
            }
            Some(EntropicState::Decayed(fields)) => {
                // Return to arena as decayed; cannot be moved/assigned as a whole
                self.bindings
                    .insert(identifier.to_string(), EntropicState::Decayed(fields));
                Err(MemoryError::StructurallyDecayed)
            }
            Some(EntropicState::Consumed) | None => {
                Err(MemoryError::AlreadyConsumed)
            }
        }
    }

    /// Extracts a single field, transitioning the parent to Decayed status.
    pub fn consume_field(
        &mut self,
        parent: &str,
        field: &str,
    ) -> Result<Payload, MemoryError> {
        let state = self
            .bindings
            .remove(parent)
            .ok_or(MemoryError::AlreadyConsumed)?;

        match state {
            EntropicState::Valid(Payload::Struct(mut fields))
            | EntropicState::Decayed(mut fields) => {
                let field_state =
                    fields.remove(field).ok_or(MemoryError::AlreadyConsumed)?;

                match field_state {
                    EntropicState::Valid(payload) => {
                        // Free only the weight of the specific field being extracted
                        self.used -= payload.weight();

                        // Mark specifically this field as consumed
                        fields.insert(field.to_string(), EntropicState::Consumed);

                        // Re-insert the parent as Decayed
                        self.bindings.insert(
                            parent.to_string(),
                            EntropicState::Decayed(fields),
                        );
                        Ok(payload)
                    }
                    _ => {
                        // Re-insert current state before failing
                        self.bindings.insert(
                            parent.to_string(),
                            EntropicState::Decayed(fields),
                        );
                        Err(MemoryError::AlreadyConsumed)
                    }
                }
            }
            _ => {
                // Re-insert non-struct state
                self.bindings.insert(parent.to_string(), state);
                Err(MemoryError::NotAStruct)
            }
        }
    }

    pub fn peek(&self, identifier: &str) -> Option<Payload> {
        match self.bindings.get(identifier) {
            Some(EntropicState::Valid(payload)) => Some(payload.clone()),
            _ => None,
        }
    }

    pub fn set_consumed(&mut self, identifier: &str) -> Result<(), MemoryError> {
        match self.bindings.get(identifier) {
            Some(EntropicState::Valid(payload)) => {
                self.used -= payload.weight();
                self.bindings
                    .insert(identifier.to_string(), EntropicState::Consumed);
                Ok(())
            }
            Some(EntropicState::Decayed(_)) | Some(EntropicState::Consumed) => {
                Err(MemoryError::AlreadyConsumed)
            }
            None => Err(MemoryError::AlreadyConsumed),
        }
    }

    pub fn decay(&mut self, identifier: &str) -> Result<(), MemoryError> {
        match self.bindings.remove(identifier) {
            Some(EntropicState::Valid(Payload::Struct(fields))) => {
                self.bindings
                    .insert(identifier.to_string(), EntropicState::Decayed(fields));
                Ok(())
            }
            Some(EntropicState::Valid(_)) => {
                self.bindings
                    .insert(identifier.to_string(), EntropicState::Consumed);
                Err(MemoryError::NotAStruct)
            }
            Some(EntropicState::Decayed(fields)) => {
                self.bindings
                    .insert(identifier.to_string(), EntropicState::Decayed(fields));
                Ok(())
            }
            _ => Err(MemoryError::AlreadyConsumed),
        }
    }

    /// Calculates the CPU and Memory overhead for cloning data.
    pub fn calculate_clone_cost(&self, payload: &Payload, depth: u32) -> u64 {
        let base_overhead = 10;
        let c_factor = 2;
        let k_factor = 5;

        base_overhead + (payload.weight() * c_factor) + (depth as u64 * k_factor)
    }
}
