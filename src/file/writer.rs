use flate2::write::GzEncoder;
use flate2::Compression;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

/// Buffered file writer supporting optional gzip compression
pub struct Writer {
  writer: Box<dyn Write>,
}

impl Writer {
  /// Creates a new file writer given a `Path`. If the path ends in `.gz`, it will encode as
  /// a gzipped file using `flate2`.
  /// 
  /// Returns `Err` if there is a problem creating the file 
  pub fn new<P>(path: P) -> io::Result<Writer>
  where
    P: AsRef<Path>
  {
      let file = File::create(&path)?;

      let writer: Box<dyn Write> = if path.as_ref().extension() == Some(&OsStr::new("gz")) {
          Box::new(GzEncoder::new(file, Compression::default()))
      } else {
          Box::new(BufWriter::new(file))
      };

      Ok(Writer { writer })
  }

  /// Appends a string `line` with a newline character (`\n`) at the end
  /// 
  /// Returns `Err` if there is a problem writing to the file
  pub fn write_line(&mut self, line: &str) -> io::Result<()> {
      self.writer.write_all(line.as_bytes())?;
      self.writer.write_all(b"\n")?;
      Ok(())
  }
}