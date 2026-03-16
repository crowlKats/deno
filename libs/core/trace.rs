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

use serde::Deserialize;
use serde::Serialize;

const TRACE_MAGIC: &[u8; 9] = b"DENO_REC\0";
const TRACE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceHeader {
  pub version: u32,
  pub deno_version: String,
  pub v8_version: String,
  pub target: String,
  pub recorded_at: u64,
  pub entry_point: String,
  pub op_names: Vec<String>,
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
  writer: RefCell<BufWriter<std::fs::File>>,
  sequence_counter: Cell<u64>,
  #[allow(dead_code)]
  op_names: Vec<String>,
  event_count: Cell<u64>,
}

impl TraceRecorder {
  pub fn new(
    path: &Path,
    entry_point: String,
    op_names: Vec<String>,
  ) -> std::io::Result<Self> {
    let file = std::fs::File::create(path)?;
    let mut writer = BufWriter::new(file);

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
    };

    // Write magic
    writer.write_all(TRACE_MAGIC)?;
    // Write header using bincode
    bincode::serialize_into(&mut writer, &header)
      .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    writer.flush()?;

    Ok(Self {
      writer: RefCell::new(writer),
      sequence_counter: Cell::new(0),
      op_names,
      event_count: Cell::new(0),
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
    let seq = self.sequence_counter.get();
    self.sequence_counter.set(seq + 1);

    let event = TraceEvent {
      sequence_id: seq,
      op_id,
      promise_id,
      is_ok,
      payload,
    };

    let mut writer = self.writer.borrow_mut();
    if let Err(e) =
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

    self.event_count.set(self.event_count.get() + 1);
  }

  pub fn event_count(&self) -> u64 {
    self.event_count.get()
  }

  pub fn op_name(&self, op_id: u16) -> &str {
    self.op_names.get(op_id as usize).map_or("unknown", |s| s)
  }
}

/// Reads trace events for replay.
pub struct TraceReplayer {
  pub header: TraceHeader,
  events: Vec<TraceEvent>,
  position: Cell<usize>,
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
          if let bincode::ErrorKind::Io(ref io_err) = *e {
            if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
              break;
            }
          }
          return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            e,
          ));
        }
      }
    }

    Ok(Self {
      header,
      events,
      position: Cell::new(0),
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

  pub fn op_name(&self, op_id: u16) -> &str {
    self
      .header
      .op_names
      .get(op_id as usize)
      .map_or("unknown", |s| s)
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
        if let bincode::ErrorKind::Io(ref io_err) = *e {
          if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
            break;
          }
        }
        return Err(std::io::Error::new(
          std::io::ErrorKind::InvalidData,
          e,
        ));
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
  },
  Replay(PathBuf),
}
