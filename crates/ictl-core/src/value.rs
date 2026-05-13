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
    Bool(bool),
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
            Payload::Bool(b) => write!(f, "{}", b),
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
    pub fn render_decay(&self, depth: usize) -> String {
        let indent = "  ".repeat(depth);
        match self {
            Payload::Struct(fields) => {
                let mut s = "struct {".to_string();
                let mut keys: Vec<_> = fields.keys().collect();
                keys.sort();
                for k in keys {
                    s.push_str(&format!(
                        "\n{}  {}: {}",
                        indent,
                        k,
                        fields[k].render_decay(depth + 1)
                    ));
                }
                s.push_str(&format!("\n{}}}", indent));
                s
            }
            Payload::Topology(fields) => {
                let mut s = "topology {".to_string();
                let mut keys: Vec<_> = fields.keys().collect();
                keys.sort();
                for k in keys {
                    s.push_str(&format!(
                        "\n{}  {}: {}",
                        indent,
                        k,
                        fields[k].render_decay(depth + 1)
                    ));
                }
                s.push_str(&format!("\n{}}}", indent));
                s
            }
            _ => format!("{}", self),
        }
    }

    /// Deterministic size calculation for ICTL payloads
    pub fn weight(&self) -> u64 {
        match self {
            Payload::Integer(_) => 8,
            Payload::Bool(_) => 1,
            Payload::String(s) => s.len() as u64 + 24, // 24 bytes for String struct overhead
            Payload::Struct(fields) => {
                let fields_weight: u64 = fields.values().map(|s| s.weight()).sum();
                fields_weight + 48 // Overhead for HashMap and metadata
            }
            Payload::Topology(fields) => {
                let fields_weight: u64 = fields.values().map(|s| s.weight()).sum();
                fields_weight + 64 // Higher overhead for topologies
            }
            Payload::Array(elems) => {
                let total: u64 = elems.iter().map(|p| p.weight()).sum();
                total + 24 // Vec overhead
            }
            Payload::Null => 8,
        }
    }
}

impl EntropicState {
    pub fn render_decay(&self, depth: usize) -> String {
        let indent = "  ".repeat(depth);
        match self {
            EntropicState::Valid(p) => {
                format!("\x1b[1;32m[Valid]\x1b[0m {}", p.render_decay(depth))
            }
            EntropicState::Consumed => "\x1b[1;31m[Consumed]\x1b[0m".to_string(),
            EntropicState::Pending(_) => "\x1b[1;34m[Pending]\x1b[0m".to_string(),
            EntropicState::Decayed(fields) => {
                let mut s = "\x1b[1;33m[Decayed]\x1b[0m {".to_string();
                let mut keys: Vec<_> = fields.keys().collect();
                keys.sort();
                for k in keys {
                    s.push_str(&format!(
                        "\n{}  {}: {}",
                        indent,
                        k,
                        fields[k].render_decay(depth + 1)
                    ));
                }
                s.push_str(&format!("\n{}}}", indent));
                s
            }
        }
    }

    /// Calculate weight of the state including its variant overhead.
    pub fn weight(&self) -> u64 {
        match self {
            EntropicState::Valid(p) => p.weight() + 16,
            EntropicState::Decayed(fields) => {
                let fields_weight: u64 = fields.values().map(|s| s.weight()).sum();
                fields_weight + 32
            }
            EntropicState::Pending(_) => 64,
            EntropicState::Consumed => 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueMetadata {
    pub instantiated_at: u64,
    pub type_name: Option<String>,
    pub decay_after_ms: Option<u64>,
}

#[derive(Clone)]
pub struct Arena {
    pub capacity: u64,
    pub used: u64,
    pub registers: Vec<EntropicState>,
    pub metadata: Vec<Option<ValueMetadata>>,
}

impl Arena {
    pub fn new(capacity: u64) -> Self {
        Self {
            capacity,
            used: 0,
            registers: Vec::new(),
            metadata: Vec::new(),
        }
    }

    fn ensure_register(&mut self, reg: u32) {
        let idx = reg as usize;
        if idx >= self.registers.len() {
            self.registers.resize(idx + 1, EntropicState::Consumed);
            self.metadata.resize(idx + 1, None);
        }
    }

    /// Checks and reserves memory before insertion
    pub fn insert(
        &mut self,
        reg: u32,
        state: EntropicState,
    ) -> Result<(), MemoryError> {
        self.ensure_register(reg);
        let idx = reg as usize;
        let mut potential_used = self.used;

        potential_used = potential_used.saturating_sub(self.registers[idx].weight());

        let state_weight = state.weight();
        if potential_used + state_weight > self.capacity {
            return Err(MemoryError::OutOfMemory(
                state_weight,
                self.capacity.saturating_sub(potential_used),
            ));
        }

        self.used = potential_used + state_weight;
        self.registers[idx] = state;
        self.metadata[idx] = None; // Clear old metadata
        Ok(())
    }

    pub fn insert_with_metadata(
        &mut self,
        reg: u32,
        state: EntropicState,
        meta: ValueMetadata,
    ) -> Result<(), MemoryError> {
        self.insert(reg, state)?;
        self.metadata[reg as usize] = Some(meta);
        Ok(())
    }

    /// Drop all arena state immediately for deterministic bulk deallocation.
    pub fn clear(&mut self) {
        self.registers.clear();
        self.metadata.clear();
        self.used = 0;
    }

    /// Optionally compact consumed entries at branch boundaries.
    pub fn compact_consumed(&mut self) {
        // In a register VM, we don't "compact" the Vec as indices must remain stable.
        // We just recalculate used memory.
        let mut new_used = 0;
        for reg in &self.registers {
            new_used += reg.weight();
        }
        self.used = new_used;
    }

    pub fn consume(&mut self, reg: u32) -> Result<Payload, MemoryError> {
        self.ensure_register(reg);
        let idx = reg as usize;
        let state = &self.registers[idx];
        match state {
            EntropicState::Valid(payload) => {
                let payload = payload.clone();
                let old_weight = state.weight();
                let new_state = EntropicState::Consumed;
                let new_weight = new_state.weight();

                self.used = self
                    .used
                    .saturating_sub(old_weight)
                    .saturating_add(new_weight);
                self.registers[idx] = new_state;
                Ok(payload)
            }
            EntropicState::Decayed(_) => Err(MemoryError::StructurallyDecayed),
            _ => Err(MemoryError::AlreadyConsumed),
        }
    }

    /// Moves the entropic state out of the arena, replacing it with Consumed.
    pub fn consume_entropic(
        &mut self,
        reg: u32,
    ) -> Result<EntropicState, MemoryError> {
        self.ensure_register(reg);
        let idx = reg as usize;
        let state =
            std::mem::replace(&mut self.registers[idx], EntropicState::Consumed);

        if matches!(state, EntropicState::Consumed) {
            return Err(MemoryError::AlreadyConsumed);
        }

        let old_weight = state.weight();
        let new_state = EntropicState::Consumed;
        let new_weight = new_state.weight();

        self.used = self
            .used
            .saturating_sub(old_weight)
            .saturating_add(new_weight);
        Ok(state)
    }

    pub fn consume_field(
        &mut self,
        reg: u32,
        field: &str,
    ) -> Result<Payload, MemoryError> {
        match self.consume_field_entropic(reg, field)? {
            EntropicState::Valid(p) => Ok(p),
            EntropicState::Decayed(_) => Err(MemoryError::StructurallyDecayed),
            _ => Err(MemoryError::AlreadyConsumed),
        }
    }

    pub fn consume_field_entropic(
        &mut self,
        reg: u32,
        field: &str,
    ) -> Result<EntropicState, MemoryError> {
        self.ensure_register(reg);
        let idx = reg as usize;
        let state =
            std::mem::replace(&mut self.registers[idx], EntropicState::Consumed);

        if matches!(state, EntropicState::Consumed) {
            return Err(MemoryError::AlreadyConsumed);
        }

        let old_parent_weight = state.weight();

        match state {
            EntropicState::Valid(Payload::Struct(mut fields))
            | EntropicState::Valid(Payload::Topology(mut fields))
            | EntropicState::Decayed(mut fields) => {
                let field_state = fields
                    .remove(field)
                    .ok_or(MemoryError::KeyNotFound(field.to_string()))?;

                // Mark specifically this field as consumed
                fields.insert(field.to_string(), EntropicState::Consumed);

                // Re-insert the parent as Decayed
                let new_state = EntropicState::Decayed(fields);
                let new_parent_weight = new_state.weight();
                self.used = self
                    .used
                    .saturating_sub(old_parent_weight)
                    .saturating_add(new_parent_weight);
                self.registers[idx] = new_state;
                Ok(field_state)
            }
            _ => {
                // Re-insert non-struct state
                self.registers[idx] = state;
                Err(MemoryError::NotAStruct)
            }
        }
    }

    pub fn peek(&self, reg: u32) -> Option<Payload> {
        let idx = reg as usize;
        if idx >= self.registers.len() {
            return None;
        }
        match &self.registers[idx] {
            EntropicState::Valid(payload) => Some(payload.clone()),
            EntropicState::Decayed(fields) => {
                // Return as a Struct payload; some internal fields may be Consumed
                Some(Payload::Struct(fields.clone()))
            }
            _ => None,
        }
    }

    pub fn set_consumed(&mut self, reg: u32) -> Result<(), MemoryError> {
        self.ensure_register(reg);
        let idx = reg as usize;
        let state = &self.registers[idx];
        let old_weight = state.weight();
        let new_state = EntropicState::Consumed;
        let new_weight = new_state.weight();
        self.used = self
            .used
            .saturating_sub(old_weight)
            .saturating_add(new_weight);
        self.registers[idx] = new_state;
        Ok(())
    }

    pub fn decay(&mut self, reg: u32) -> Result<(), MemoryError> {
        self.ensure_register(reg);
        let idx = reg as usize;
        let state =
            std::mem::replace(&mut self.registers[idx], EntropicState::Consumed);
        let old_weight = state.weight();

        let new_state = match state {
            EntropicState::Valid(Payload::Struct(fields)) => {
                EntropicState::Decayed(fields)
            }
            EntropicState::Valid(_) => EntropicState::Consumed,
            EntropicState::Decayed(fields) => EntropicState::Decayed(fields),
            _ => EntropicState::Consumed,
        };

        let new_weight = new_state.weight();
        self.used = self
            .used
            .saturating_sub(old_weight)
            .saturating_add(new_weight);
        self.registers[idx] = new_state;
        Ok(())
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
        reg: u32,
        field: &str,
        new_value: Payload,
    ) -> Result<(), MemoryError> {
        self.ensure_register(reg);
        let idx = reg as usize;
        let state =
            std::mem::replace(&mut self.registers[idx], EntropicState::Consumed);

        if matches!(state, EntropicState::Consumed) {
            return Err(MemoryError::AlreadyConsumed);
        }

        let old_parent_weight = state.weight();
        let is_topology =
            matches!(state, EntropicState::Valid(Payload::Topology(_)));
        let is_struct = matches!(state, EntropicState::Valid(Payload::Struct(_)));

        match state {
            EntropicState::Valid(Payload::Struct(mut fields))
            | EntropicState::Valid(Payload::Topology(mut fields))
            | EntropicState::Decayed(mut fields) => {
                fields.insert(field.to_string(), EntropicState::Valid(new_value));

                let new_state = if is_struct {
                    EntropicState::Valid(Payload::Struct(fields))
                } else if is_topology {
                    EntropicState::Valid(Payload::Topology(fields))
                } else {
                    EntropicState::Decayed(fields)
                };

                let new_parent_weight = new_state.weight();
                if self.used.saturating_sub(old_parent_weight) + new_parent_weight
                    > self.capacity
                {
                    self.registers[idx] = EntropicState::Consumed;
                    return Err(MemoryError::OutOfMemory(
                        new_parent_weight,
                        self.capacity - (self.used - old_parent_weight),
                    ));
                }

                self.used = self
                    .used
                    .saturating_sub(old_parent_weight)
                    .saturating_add(new_parent_weight);
                self.registers[idx] = new_state;
                Ok(())
            }
            _ => {
                self.registers[idx] = state;
                Err(MemoryError::NotAStruct)
            }
        }
    }

    pub fn update_index_field(
        &mut self,
        reg: u32,
        index: &str,
        field: &str,
        new_value: Payload,
    ) -> Result<(), MemoryError> {
        println!("[DEBUG] Arena::update_index_field reg={}, index={}, field={}, value={:?}", reg, index, field, new_value);
        self.ensure_register(reg);
        let idx = reg as usize;
        let state =
            std::mem::replace(&mut self.registers[idx], EntropicState::Consumed);

        if matches!(state, EntropicState::Consumed) {
            return Err(MemoryError::AlreadyConsumed);
        }

        let old_parent_weight = state.weight();

        match state {
            EntropicState::Valid(Payload::Topology(mut fields)) => {
                let inner_state =
                    fields.get_mut(index).ok_or(MemoryError::AlreadyConsumed)?;
                match inner_state {
                    EntropicState::Valid(Payload::Struct(inner_fields))
                    | EntropicState::Valid(Payload::Topology(inner_fields)) => {
                        println!("[DEBUG] Found inner struct/topology at index {}, updating field {}", index, field);
                        inner_fields.insert(
                            field.to_string(),
                            EntropicState::Valid(new_value),
                        );
                    }
                    _ => {
                        self.registers[idx] =
                            EntropicState::Valid(Payload::Topology(fields));
                        return Err(MemoryError::NotAStruct);
                    }
                }

                let new_state = EntropicState::Valid(Payload::Topology(fields));
                println!("[DEBUG] Successfully updated nested field");

                let new_parent_weight = new_state.weight();
                if self.used.saturating_sub(old_parent_weight) + new_parent_weight
                    > self.capacity
                {
                    self.registers[idx] = EntropicState::Consumed;
                    return Err(MemoryError::OutOfMemory(
                        new_parent_weight,
                        self.capacity
                            - (self.used.saturating_sub(old_parent_weight)),
                    ));
                }

                self.used = self
                    .used
                    .saturating_sub(old_parent_weight)
                    .saturating_add(new_parent_weight);
                self.registers[idx] = new_state;
                Ok(())
            }
            _ => {
                self.registers[idx] = state;
                Err(MemoryError::NotAStruct)
            }
        }
    }

    pub fn update_deep_field(
        &mut self,
        reg: u32,
        path: &[String],
        new_value: Payload,
    ) -> Result<(), MemoryError> {
        if path.len() == 1 {
            return self.update_field(reg, &path[0], new_value);
        }

        self.ensure_register(reg);
        let idx = reg as usize;
        let state =
            std::mem::replace(&mut self.registers[idx], EntropicState::Consumed);

        if matches!(state, EntropicState::Consumed) {
            return Err(MemoryError::AlreadyConsumed);
        }

        let old_weight = state.weight();
        let is_topology =
            matches!(state, EntropicState::Valid(Payload::Topology(_)));
        let is_struct = matches!(state, EntropicState::Valid(Payload::Struct(_)));

        match state {
            EntropicState::Valid(Payload::Struct(mut fields))
            | EntropicState::Valid(Payload::Topology(mut fields))
            | EntropicState::Decayed(mut fields) => {
                Self::deep_set(&mut fields, path, new_value)?;

                let final_state = if is_struct {
                    EntropicState::Valid(Payload::Struct(fields))
                } else if is_topology {
                    EntropicState::Valid(Payload::Topology(fields))
                } else {
                    EntropicState::Decayed(fields)
                };

                let new_weight = final_state.weight();
                if self.used.saturating_sub(old_weight) + new_weight > self.capacity
                {
                    self.registers[idx] = EntropicState::Consumed;
                    return Err(MemoryError::OutOfMemory(
                        new_weight,
                        self.capacity - (self.used - old_weight),
                    ));
                }

                self.used = self
                    .used
                    .saturating_sub(old_weight)
                    .saturating_add(new_weight);
                self.registers[idx] = final_state;
                Ok(())
            }
            _ => {
                self.registers[idx] = state;
                Err(MemoryError::NotAStruct)
            }
        }
    }

    fn deep_set(
        fields: &mut HashMap<String, EntropicState>,
        path: &[String],
        new_value: Payload,
    ) -> Result<(), MemoryError> {
        if path.is_empty() {
            return Err(MemoryError::KeyNotFound("empty path".to_string()));
        }

        if path.len() == 1 {
            fields.insert(path[0].clone(), EntropicState::Valid(new_value));
            return Ok(());
        }

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
