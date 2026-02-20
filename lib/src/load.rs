use arrow::{
    array::RecordBatch,
    compute::concat_batches,
    csv::{ReaderBuilder, reader::Format},
    error::ArrowError,
};
use memmap2::Mmap;
use std::{
    fs::File,
    io::{self, BufRead, BufReader, Cursor, Read, Seek},
    path::Path,
    sync::Arc,
};

#[derive(Debug)]
pub enum LoadError {
    FileNotFound,
    FileNotUnderstood(Option<String>),
    Io(io::Error),
    ArrowError(Option<String>),
}

impl From<io::Error> for LoadError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<ArrowError> for LoadError {
    fn from(value: ArrowError) -> Self {
        match value {
            ArrowError::IoError(_, e) => Self::Io(e),

            ArrowError::SchemaError(e)
            | ArrowError::CsvError(e)
            | ArrowError::JsonError(e)
            | ArrowError::IpcError(e)
            | ArrowError::ParquetError(e) => Self::FileNotUnderstood(Some(e)),

            ArrowError::NotYetImplemented(e)
            | ArrowError::CastError(e)
            | ArrowError::MemoryError(e)
            | ArrowError::ParseError(e)
            | ArrowError::ComputeError(e)
            | ArrowError::ArithmeticOverflow(e)
            | ArrowError::AvroError(e)
            | ArrowError::InvalidArgumentError(e)
            | ArrowError::CDataInterface(e) => Self::ArrowError(Some(e)),

            ArrowError::ExternalError(e) => Self::ArrowError(Some(e.to_string())),

            _ => Self::ArrowError(None),
        }
    }
}

struct DelimitedFormat {
    format: Format,
    preamble_rows: u64,
}

fn sniff_delimited<R: Read + Seek>(data: R) -> Result<DelimitedFormat, LoadError> {
    let metadata = csv_nose::Sniffer::new()
        .sniff_reader(data)
        .map_err(|e| match e {
            csv_nose::SnifferError::Io(e) => LoadError::from(e),
            csv_nose::SnifferError::InvalidConfig(e) => unreachable!("Invalid sniff config: {e}"),
            _ => LoadError::FileNotUnderstood(Some(e.to_string())),
        })?;

    let mut format = Format::default()
        .with_delimiter(metadata.dialect.delimiter)
        .with_header(metadata.dialect.header.has_header_row)
        .with_truncated_rows(metadata.dialect.flexible);

    if let Some(quote_char) = metadata.dialect.quote.char() {
        format = format.with_quote(quote_char);
    };

    Ok(DelimitedFormat {
        format,
        preamble_rows: metadata.dialect.header.num_preamble_rows as u64,
    })
}

fn open_delimited<R: Read>(
    schema_handle: R,
    read_handle: R,
    format: DelimitedFormat,
) -> Result<RecordBatch, LoadError> {
    let DelimitedFormat {
        format,
        preamble_rows,
    } = format;

    // Read out `preamble_rows` from buffers
    let (mut schema_handle, mut read_handle) =
        (BufReader::new(schema_handle), BufReader::new(read_handle));
    if preamble_rows != 0 {
        let mut _throwaway = String::new();
        for _ in 0..preamble_rows {
            schema_handle
                .read_line(&mut _throwaway)
                .map_err(LoadError::from)?;
            _throwaway.clear();
            read_handle
                .read_line(&mut _throwaway)
                .map_err(LoadError::from)?;
            _throwaway.clear();
        }
    }

    let (schema, _len) = format
        .infer_schema(schema_handle, Some(100))
        .map_err(LoadError::from)?;
    let schema_arc = Arc::new(schema);

    let reader = ReaderBuilder::new(schema_arc.clone())
        .with_format(format)
        .build_buffered(read_handle)
        .map_err(LoadError::from)?;
    let batches = reader
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(LoadError::from)?;

    concat_batches(&schema_arc, batches.iter()).map_err(LoadError::from)
}

pub fn load_delimited_file<P: AsRef<Path>>(path: P) -> Result<RecordBatch, LoadError> {
    fn inner(path: &Path) -> Result<RecordBatch, LoadError> {
        if !path.exists() {
            return Err(LoadError::FileNotFound);
        }

        let file = File::open(path).map_err(LoadError::from)?;
        let mmap = unsafe { Mmap::map(&file) }.map_err(LoadError::from)?;

        let sniff_reader = Cursor::new(&mmap);
        let format = sniff_delimited(sniff_reader)?;

        let schema_reader = Cursor::new(&mmap);
        let file_reader = Cursor::new(&mmap);
        let batch = open_delimited(schema_reader, file_reader, format)?;

        Ok(batch)
    }

    inner(path.as_ref())
}
