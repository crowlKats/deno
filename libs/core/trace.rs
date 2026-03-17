// Copyright 2018-2026 the Deno authors. MIT license.

//! Recording and replay trace format for deterministic execution.
//!
//! The trace captures async op results during execution and can replay them
//! to reproduce the exact same observable behavior from JavaScript's perspective.

use std::cell::Cell;
use std::cell::RefCell;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

use serde::Deserialize;
use serde::Serialize;

const TRACE_MAGIC: &[u8; 9] = b"DENO_REC\0";
const TRACE_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceHeader {
  pub version: u32,
  pub deno_version: String,
  pub v8_version: String,
  pub target: String,
  pub recorded_at: u64,
  pub entry_point: String,
  pub op_names: Vec<String>,
  /// Seed used for Math.random and crypto.getRandomValues determinism.
  /// If None during recording, a random seed was auto-generated.
  pub seed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
  pub sequence_id: u64,
  pub op_id: u16,
  pub promise_id: i32,
  pub is_ok: bool,
  /// JSON-serialized V8 value.
  pub payload: Vec<u8>,
}

/// Records async op results to a trace file during execution.
pub struct TraceRecorder {
  /// In streaming mode (no limit), writes directly to file.
  /// In ring buffer mode, this is None until flush.
  writer: RefCell<Option<BufWriter<std::fs::File>>>,
  header: TraceHeader,
  path: PathBuf,
  sequence_counter: Cell<u64>,
  op_names: Vec<String>,
  event_count: Cell<u64>,
  /// Ring buffer for --record-limit mode.
  ring_buffer: RefCell<Option<std::collections::VecDeque<TraceEvent>>>,
  ring_limit: Option<usize>,
  /// Op name filter — only record ops whose names are in this set.
  filter: Option<Vec<String>>,
}

impl TraceRecorder {
  pub fn new(
    path: &Path,
    entry_point: String,
    op_names: Vec<String>,
    seed: Option<u64>,
    limit: Option<usize>,
    filter: Option<Vec<String>>,
  ) -> std::io::Result<Self> {
    let header = TraceHeader {
      version: TRACE_VERSION,
      deno_version: env!("CARGO_PKG_VERSION").to_string(),
      v8_version: v8::VERSION_STRING.to_string(),
      target: std::env::consts::ARCH.to_string(),
      recorded_at: std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64,
      entry_point,
      op_names: op_names.clone(),
      seed,
    };

    let writer = if limit.is_some() {
      // In ring buffer mode, defer writing until flush
      None
    } else {
      let file = std::fs::File::create(path)?;
      let mut writer = BufWriter::new(file);
      writer.write_all(TRACE_MAGIC)?;
      bincode::serialize_into(&mut writer, &header)
        .map_err(std::io::Error::other)?;
      writer.flush()?;
      Some(writer)
    };

    let ring_buffer = limit.map(std::collections::VecDeque::with_capacity);

    Ok(Self {
      writer: RefCell::new(writer),
      header,
      path: path.to_path_buf(),
      sequence_counter: Cell::new(0),
      op_names,
      event_count: Cell::new(0),
      ring_buffer: RefCell::new(ring_buffer),
      ring_limit: limit,
      filter,
    })
  }

  /// Record an async op result.
  pub fn record_event(
    &self,
    op_id: u16,
    promise_id: i32,
    is_ok: bool,
    payload: Vec<u8>,
  ) {
    // Apply op name filter
    if let Some(ref filter) = self.filter {
      let name = self.op_name(op_id);
      if !filter.iter().any(|f| name.contains(f.as_str())) {
        return;
      }
    }

    let seq = self.sequence_counter.get();
    self.sequence_counter.set(seq + 1);

    let event = TraceEvent {
      sequence_id: seq,
      op_id,
      promise_id,
      is_ok,
      payload,
    };

    if let Some(limit) = self.ring_limit {
      // Ring buffer mode — keep in memory
      let mut ring = self.ring_buffer.borrow_mut();
      if let Some(ref mut buf) = *ring {
        if buf.len() >= limit {
          buf.pop_front();
        }
        buf.push_back(event);
      }
    } else {
      // Streaming mode — write directly
      let mut writer_opt = self.writer.borrow_mut();
      if let Some(ref mut writer) = *writer_opt
        && let Err(e) =
          bincode::serialize_into(&mut *writer, &event).and_then(|_| {
            writer
              .flush()
              .map_err(|e| Box::new(bincode::ErrorKind::Io(e)))
          })
      {
        #[allow(clippy::print_stderr)]
        {
          eprintln!("Warning: failed to write trace event: {e}");
        }
      }
    }

    self.event_count.set(self.event_count.get() + 1);
  }

  /// Flush ring buffer to disk. Called on drop for ring buffer mode.
  fn flush_ring_buffer(&self) {
    let ring = self.ring_buffer.borrow();
    let Some(ref buf) = *ring else { return };

    let result = (|| -> std::io::Result<()> {
      let file = std::fs::File::create(&self.path)?;
      let mut writer = BufWriter::new(file);
      writer.write_all(TRACE_MAGIC)?;
      bincode::serialize_into(&mut writer, &self.header)
        .map_err(std::io::Error::other)?;
      for event in buf.iter() {
        bincode::serialize_into(&mut writer, event)
          .map_err(std::io::Error::other)?;
      }
      writer.flush()?;
      Ok(())
    })();

    if let Err(e) = result {
      #[allow(clippy::print_stderr)]
      {
        eprintln!("Warning: failed to flush trace ring buffer: {e}");
      }
    }
  }

  pub fn event_count(&self) -> u64 {
    self.event_count.get()
  }

  pub fn op_name(&self, op_id: u16) -> &str {
    self.op_names.get(op_id as usize).map_or("unknown", |s| s)
  }
}

impl Drop for TraceRecorder {
  fn drop(&mut self) {
    if self.ring_limit.is_some() {
      self.flush_ring_buffer();
    }
  }
}

/// Reads trace events for replay.
pub struct TraceReplayer {
  pub header: TraceHeader,
  events: Vec<TraceEvent>,
  position: Rc<Cell<usize>>,
}

impl TraceReplayer {
  pub fn from_file(path: &Path) -> std::io::Result<Self> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);

    // Read and verify magic
    let mut magic = [0u8; 9];
    reader.read_exact(&mut magic)?;
    if &magic != TRACE_MAGIC {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Not a valid Deno trace file",
      ));
    }

    // Read header
    let header: TraceHeader = bincode::deserialize_from(&mut reader)
      .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    if header.version != TRACE_VERSION {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!(
          "Trace version mismatch: expected {}, got {}",
          TRACE_VERSION, header.version
        ),
      ));
    }

    // Read all events
    let mut events = Vec::new();
    loop {
      match bincode::deserialize_from::<_, TraceEvent>(&mut reader) {
        Ok(event) => events.push(event),
        Err(e) => {
          if let bincode::ErrorKind::Io(ref io_err) = *e
            && io_err.kind() == std::io::ErrorKind::UnexpectedEof
          {
            break;
          }
          return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e));
        }
      }
    }

    Ok(Self {
      header,
      events,
      position: Rc::new(Cell::new(0)),
    })
  }

  /// Get the next replay event, if any.
  pub fn next_event(&self) -> Option<&TraceEvent> {
    let pos = self.position.get();
    if pos < self.events.len() {
      self.position.set(pos + 1);
      Some(&self.events[pos])
    } else {
      None
    }
  }

  /// Peek at the next event without consuming it.
  pub fn peek_event(&self) -> Option<&TraceEvent> {
    let pos = self.position.get();
    self.events.get(pos)
  }

  /// Check if there are more events to replay.
  pub fn has_more(&self) -> bool {
    self.position.get() < self.events.len()
  }

  /// Total number of events in the trace.
  pub fn event_count(&self) -> usize {
    self.events.len()
  }

  /// Current position in the replay.
  pub fn position(&self) -> usize {
    self.position.get()
  }

  /// Skip to a specific position in the replay.
  pub fn seek_to(&self, position: usize) {
    self.position.set(position.min(self.events.len()));
  }

  pub fn op_name(&self, op_id: u16) -> &str {
    self
      .header
      .op_names
      .get(op_id as usize)
      .map_or("unknown", |s| s)
  }

  /// Create inspector info sharing the position with this replayer.
  pub fn inspector_info(&self) -> crate::inspector::ReplayInspectorInfo {
    crate::inspector::ReplayInspectorInfo {
      header: self.header.clone(),
      events: self.events.clone(),
      position: self.position.clone(),
    }
  }
}

/// Read trace metadata without loading all events.
pub fn read_trace_info(path: &Path) -> std::io::Result<TraceInfo> {
  let file = std::fs::File::open(path)?;
  let mut reader = BufReader::new(file);

  let mut magic = [0u8; 9];
  reader.read_exact(&mut magic)?;
  if &magic != TRACE_MAGIC {
    return Err(std::io::Error::new(
      std::io::ErrorKind::InvalidData,
      "Not a valid Deno trace file",
    ));
  }

  let header: TraceHeader = bincode::deserialize_from(&mut reader)
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

  // Count events
  let mut event_count: u64 = 0;
  loop {
    match bincode::deserialize_from::<_, TraceEvent>(&mut reader) {
      Ok(_) => event_count += 1,
      Err(e) => {
        if let bincode::ErrorKind::Io(ref io_err) = *e
          && io_err.kind() == std::io::ErrorKind::UnexpectedEof
        {
          break;
        }
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, e));
      }
    }
  }

  let file_size = std::fs::metadata(path)?.len();

  Ok(TraceInfo {
    header,
    event_count,
    file_size,
  })
}

#[derive(Debug)]
pub struct TraceInfo {
  pub header: TraceHeader,
  pub event_count: u64,
  pub file_size: u64,
}

/// Configuration for trace recording/replaying passed to the runtime.
#[derive(Debug, Clone)]
pub enum TraceMode {
  Record {
    path: PathBuf,
    entry_point: String,
    seed: Option<u64>,
    /// Keep only the last N events (ring buffer mode).
    limit: Option<usize>,
    /// Only record ops matching these names.
    filter: Option<Vec<String>>,
  },
  Replay {
    path: PathBuf,
    /// Skip to the Nth event before starting replay.
    seek: Option<u64>,
    /// Replay speed multiplier (e.g., 2.0 for double speed).
    speed: Option<f64>,
  },
}
