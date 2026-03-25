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
    #[error("Key not found in topology: {0}")]
    KeyNotFound(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPromise {
    pub capability: String,
    pub params: HashMap<String, String>,
    pub requested_at: u64,
    pub ready_at: u64,
    pub deadline_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntropicState {
    Valid(Payload),
    Decayed(HashMap<String, EntropicState>),
    Pending(PendingPromise),
    Consumed,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    Integer(i64),
    String(String),
    Struct(HashMap<String, EntropicState>),
    Topology(HashMap<String, EntropicState>),
    Array(Vec<Payload>),
    Null,
}

impl std::fmt::Display for Payload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Payload::Integer(i) => write!(f, "{}", i),
            Payload::String(s) => write!(f, "{}", s),
            Payload::Struct(fields) => {
                let mut pairs: Vec<String> = Vec::new();
                for (k, v) in fields {
                    let s = match v {
                        EntropicState::Valid(p) => format!("{}: {}", k, p),
                        EntropicState::Decayed(map) => {
                            let fields: Vec<String> = map
                                .iter()
                                .map(|(k2, v2)| match v2 {
                                    EntropicState::Valid(p2) => {
                                        format!("{}: {}", k2, p2)
                                    }
                                    _ => format!("{}: <decayed>", k2),
                                })
                                .collect();
                            format!("{}: {{ {} }}", k, fields.join(", "))
                        }
                        EntropicState::Pending(_) => format!("{}: <pending>", k),
                        EntropicState::Consumed => format!("{}: <consumed>", k),
                    };
                    pairs.push(s);
                }
                write!(f, "struct{{{}}}", pairs.join(", "))
            }
            Payload::Topology(fields) => {
                let mut pairs: Vec<String> = Vec::new();
                for (k, v) in fields {
                    let s = match v {
                        EntropicState::Valid(p) => format!("{}: {}", k, p),
                        EntropicState::Decayed(_map) => format!("{}: Decayed", k),
                        EntropicState::Pending(_) => format!("{}: Pending", k),
                        EntropicState::Consumed => format!("{}: Consumed", k),
                    };
                    pairs.push(s);
                }
                write!(f, "topology {{ {} }}", pairs.join(", "))
            }
            Payload::Array(elems) => {
                let strings: Vec<String> =
                    elems.iter().map(|e| format!("{}", e)).collect();
                write!(f, "[{}]", strings.join(", "))
            }
            Payload::Null => write!(f, "null"),
        }
    }
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
            Payload::Topology(fields) => {
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
                fields_weight + 32 // Higher overhead for topologies
            }
            Payload::Array(elems) => {
                let total: u64 = elems.iter().map(|p| p.weight()).sum();
                total + 16
            }
            Payload::Null => 8,
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
        // Subtract previous value if it was Valid, to support rebinding/updating
        if let Some(previous) = self.bindings.get(&identifier) {
            if let EntropicState::Valid(prev_payload) = previous {
                self.used = self.used.saturating_sub(prev_payload.weight());
            }
        }

        if let EntropicState::Valid(ref p) = state {
            let weight = p.weight();
            if self.used + weight > self.capacity {
                // Restore previous weight on failure
                if let Some(previous) = self.bindings.get(&identifier) {
                    if let EntropicState::Valid(prev_payload) = previous {
                        self.used += prev_payload.weight();
                    }
                }
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

    /// Drop all arena state immediately for deterministic bulk deallocation.
    pub fn clear(&mut self) {
        self.bindings.clear();
        self.used = 0;
    }

    /// Optionally compact consumed entries at branch boundaries.
    pub fn compact_consumed(&mut self) {
        self.bindings
            .retain(|_, v| !matches!(v, EntropicState::Consumed));
    }

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
            Some(EntropicState::Pending(_)) => Err(MemoryError::AlreadyConsumed),
            Some(EntropicState::Consumed) | None => {
                Err(MemoryError::AlreadyConsumed)
            }
        }
    }

    /// Moves the entropic state out of the arena, replacing it with Consumed.
    pub fn consume_entropic(
        &mut self,
        identifier: &str,
    ) -> Result<EntropicState, MemoryError> {
        let state = self
            .bindings
            .remove(identifier)
            .ok_or(MemoryError::AlreadyConsumed)?;
        if let EntropicState::Valid(ref p) = state {
            self.used = self.used.saturating_sub(p.weight());
        }
        self.bindings
            .insert(identifier.to_string(), EntropicState::Consumed);
        Ok(state)
    }

    pub fn consume_field(
        &mut self,
        parent: &str,
        field: &str,
    ) -> Result<Payload, MemoryError> {
        match self.consume_field_entropic(parent, field)? {
            EntropicState::Valid(p) => Ok(p),
            EntropicState::Decayed(_) => Err(MemoryError::StructurallyDecayed),
            _ => Err(MemoryError::AlreadyConsumed),
        }
    }

    pub fn consume_field_entropic(
        &mut self,
        parent: &str,
        field: &str,
    ) -> Result<EntropicState, MemoryError> {
        let state = self
            .bindings
            .remove(parent)
            .ok_or(MemoryError::AlreadyConsumed)?;

        match state {
            EntropicState::Valid(Payload::Struct(mut fields))
            | EntropicState::Valid(Payload::Topology(mut fields))
            | EntropicState::Decayed(mut fields) => {
                let field_state = fields
                    .remove(field)
                    .ok_or(MemoryError::KeyNotFound(field.to_string()))?;

                if let EntropicState::Valid(ref p) = field_state {
                    self.used = self.used.saturating_sub(p.weight());
                }

                // Mark specifically this field as consumed
                fields.insert(field.to_string(), EntropicState::Consumed);

                // Re-insert the parent as Decayed
                self.bindings
                    .insert(parent.to_string(), EntropicState::Decayed(fields));
                Ok(field_state)
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
            Some(EntropicState::Decayed(fields)) => {
                // Return as a Struct payload; some internal fields may be Consumed
                Some(Payload::Struct(fields.clone()))
            }
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
            Some(EntropicState::Pending(_)) => {
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

    pub fn update_field(
        &mut self,
        parent: &str,
        field: &str,
        new_value: Payload,
    ) -> Result<(), MemoryError> {
        let state = self
            .bindings
            .remove(parent)
            .ok_or(MemoryError::AlreadyConsumed)?;

        let is_topology =
            matches!(state, EntropicState::Valid(Payload::Topology(_)));
        let is_struct = matches!(state, EntropicState::Valid(Payload::Struct(_)));

        match state {
            EntropicState::Valid(Payload::Struct(mut fields))
            | EntropicState::Valid(Payload::Topology(mut fields))
            | EntropicState::Decayed(mut fields) => {
                let prev = fields.get(field);
                if let Some(EntropicState::Valid(p)) = prev {
                    self.used = self.used.saturating_sub(p.weight());
                }

                let weight = new_value.weight();
                if self.used + weight > self.capacity {
                    // Restore
                    let restored = if is_struct {
                        EntropicState::Valid(Payload::Struct(fields))
                    } else if is_topology {
                        EntropicState::Valid(Payload::Topology(fields))
                    } else {
                        EntropicState::Decayed(fields)
                    };
                    self.bindings.insert(parent.to_string(), restored);
                    return Err(MemoryError::OutOfMemory(
                        weight,
                        self.capacity - self.used,
                    ));
                }
                self.used += weight;

                fields.insert(field.to_string(), EntropicState::Valid(new_value));
                // Re-insert
                let final_state = if is_struct {
                    EntropicState::Valid(Payload::Struct(fields))
                } else if is_topology {
                    EntropicState::Valid(Payload::Topology(fields))
                } else {
                    EntropicState::Decayed(fields)
                };
                self.bindings.insert(parent.to_string(), final_state);
                Ok(())
            }
            _ => {
                self.bindings.insert(parent.to_string(), state);
                Err(MemoryError::NotAStruct)
            }
        }
    }

    /// Update a deeply nested field in-place.
    /// `path` is a sequence of keys: for `graph["core"].status = X`,
    /// root = "graph", path = ["core", "status"].
    pub fn update_deep_field(
        &mut self,
        root: &str,
        path: &[String],
        new_value: Payload,
    ) -> Result<(), MemoryError> {
        if path.len() == 1 {
            // Simple single-level update, delegate to existing method
            return self.update_field(root, &path[0], new_value);
        }

        let state = self
            .bindings
            .remove(root)
            .ok_or(MemoryError::AlreadyConsumed)?;

        let is_topology =
            matches!(state, EntropicState::Valid(Payload::Topology(_)));
        let is_struct = matches!(state, EntropicState::Valid(Payload::Struct(_)));

        match state {
            EntropicState::Valid(Payload::Struct(mut fields))
            | EntropicState::Valid(Payload::Topology(mut fields))
            | EntropicState::Decayed(mut fields) => {
                // Navigate into nested fields
                Self::deep_set(&mut fields, &path, new_value)?;

                // Re-insert with correct variant
                let final_state = if is_struct {
                    EntropicState::Valid(Payload::Struct(fields))
                } else if is_topology {
                    EntropicState::Valid(Payload::Topology(fields))
                } else {
                    EntropicState::Decayed(fields)
                };
                self.bindings.insert(root.to_string(), final_state);
                Ok(())
            }
            _ => {
                self.bindings.insert(root.to_string(), state);
                Err(MemoryError::NotAStruct)
            }
        }
    }

    /// Recursively navigate into nested HashMap fields and set the leaf value.
    fn deep_set(
        fields: &mut HashMap<String, EntropicState>,
        path: &[String],
        new_value: Payload,
    ) -> Result<(), MemoryError> {
        if path.is_empty() {
            return Err(MemoryError::KeyNotFound("empty path".to_string()));
        }

        if path.len() == 1 {
            // Leaf update
            fields.insert(path[0].clone(), EntropicState::Valid(new_value));
            return Ok(());
        }

        // Navigate deeper
        let key = &path[0];
        let entry = fields
            .get_mut(key)
            .ok_or(MemoryError::KeyNotFound(key.clone()))?;

        match entry {
            EntropicState::Valid(Payload::Struct(inner))
            | EntropicState::Valid(Payload::Topology(inner))
            | EntropicState::Decayed(inner) => {
                Self::deep_set(inner, &path[1..], new_value)
            }
            _ => Err(MemoryError::NotAStruct),
        }
    }
}
