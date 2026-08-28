use std::{
    fs::File,
    io::{self, BufRead, BufReader, Cursor, Read, Seek},
    path::Path,
};

use arrow::{
    array::RecordBatch,
    compute::concat_batches,
    csv::{ReaderBuilder, reader::Format},
    error::ArrowError,
    ipc::{
        CompressionType,
        reader::StreamReader,
        writer::{IpcWriteOptions, StreamWriter},
    },
    util::display::{ArrayFormatter, FormatOptions},
};
use dex_core::prelude::*;
use egui::{Align, Color32, Layout, TextStyle};
use egui_extras::{Column, TableBuilder};
use memmap2::Mmap;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _, ser::Error as _};

use crate::layouts::{Bordered, LayoutChild, ScrollLayout};

/// A [`RecordBatch`] wrapped with (de)serialization and construction features.
#[derive(Clone, Debug)]
pub struct ArrowData(pub RecordBatch);

impl ArrowData {
    /// Encode the batch as a Zstd-compressed IPC stream.
    fn to_ipc(&self) -> Result<Vec<u8>, ArrowError> {
        let options =
            IpcWriteOptions::default().try_with_compression(Some(CompressionType::ZSTD))?;
        let mut buf = Vec::new();
        let mut writer = StreamWriter::try_new_with_options(&mut buf, &self.0.schema(), options)?;
        writer.write(&self.0)?;
        writer.finish()?;
        drop(writer);
        Ok(buf)
    }

    /// Decode a batch from a Arrow IPC stream, concatenating any chunks back into one batch.
    fn from_ipc(bytes: &[u8]) -> Result<Self, ArrowError> {
        let reader = StreamReader::try_new(Cursor::new(bytes), None)?;
        let schema = reader.schema();
        let batches = reader.collect::<Result<Vec<_>, _>>()?;
        let batch = concat_batches(&schema, &batches)?;
        Ok(Self(batch))
    }
}

impl Serialize for ArrowData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let bytes = self.to_ipc().map_err(S::Error::custom)?;
        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for ArrowData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        Self::from_ipc(&bytes).map_err(D::Error::custom)
    }
}

utils::impl_Reset_noop!(ArrowData);

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

fn sniff_delimited<R: Read + Seek>(
    data: R,
    delimiter: Option<u8>,
) -> Result<DelimitedFormat, LoadError> {
    let mut sniffer = csv_nose::Sniffer::new();
    if let Some(delimiter) = delimiter {
        sniffer.delimiter(delimiter);
    }
    let metadata = sniffer.sniff_reader(data).map_err(|e| match e {
        csv_nose::SnifferError::Io(e) => LoadError::from(e),
        csv_nose::SnifferError::InvalidConfig(e) => LoadError::FileNotUnderstood(Some(e)),
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
        .infer_schema(schema_handle, Some(500))
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

impl ArrowData {
    fn from_data(data: &[u8], delimiter: Option<u8>) -> Result<Self, LoadError> {
        let sniff_reader = Cursor::new(&data);
        let format = sniff_delimited(sniff_reader, delimiter)?;

        let schema_reader = Cursor::new(&data);
        let file_reader = Cursor::new(&data);
        let batch = open_delimited(schema_reader, file_reader, format)?;

        Ok(Self(batch))
    }
}

/// A tabular view backed by an Apache Arrow [`RecordBatch`].
#[utils::portable]
pub struct Table {
    data: ArrowData,
    pub striped: bool,
}

impl Table {
    /// A table over an existing [`RecordBatch`].
    pub fn new(data: ArrowData) -> Self {
        Self {
            data,
            striped: true,
        }
    }

    /// The backing record batch.
    pub fn batch(&self) -> &RecordBatch {
        &self.data.0
    }

    /// Parse a delimited file (CSV/TSV/...) into a table.
    pub fn from_file(path: &Path) -> Result<Self, LoadError> {
        if !path.exists() {
            return Err(LoadError::FileNotFound);
        }

        let file = File::open(path).map_err(LoadError::from)?;
        let mmap = unsafe { Mmap::map(&file) }.map_err(LoadError::from)?;
        let data = ArrowData::from_data(&mmap, delimiter_for_ext(path))?;

        Ok(Self::new(data))
    }
}

/// The field delimiter implied by a file extension, if any.
fn delimiter_for_ext(path: &Path) -> Option<u8> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("csv") => Some(b','),
        Some("tsv") | Some("tab") => Some(b'\t'),
        Some("psv") => Some(b'|'),
        _ => None,
    }
}

#[utils::dynamic_node(skip)]
impl Node for Table {
    fn type_name(&self) -> String {
        "Table".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let widget = TableWidget {
            data: self.data.clone(),
            striped: self.striped,
            owner: ctx.node.id,
        };
        let scroll = ScrollLayout::horizontal(LayoutChild::Node(Arc::new(widget)))
            .with_id_salt(ctx.node.id);
        let bordered = Bordered {
            child: LayoutChild::Node(Arc::new(scroll)),
            padding: 6.0,
            corner_radius: 6.0,
            fill_color: Color::WHITE,
            border_width: 1.0,
            border_color: Color::gray(180),
        };
        let constraints = ctx.constraints;
        ctx.draw_node(&bordered, constraints)
    }
}

defhandlers! { Table {} }

/// The body of a [`Table`], backed by [`egui_extras::Table`].
#[utils::portable]
struct TableWidget {
    data: ArrowData,
    striped: bool,
    owner: NodeUid,
}

#[utils::dynamic_node(skip)]
impl Node for TableWidget {
    fn type_name(&self) -> String {
        "Table Body".into()
    }

    fn draw(&self, mut ctx: DrawContext) -> DrawResult {
        let batch = &self.data.0;
        let schema = batch.schema();
        let num_cols = batch.num_columns();
        let num_rows = batch.num_rows();
        if num_cols == 0 {
            return DrawResult::Complete { region: None };
        }

        // Width is unbounded due to `ScrollLayout::horizontal`: host a large box and let `auto_shrink` collapse it.
        let pos = ctx.constraints.pos;
        let width = ctx
            .constraints
            .x
            .map(|a| a.provided_value())
            .unwrap_or(100_000.0);
        let height = ctx
            .constraints
            .y
            .map(|a| a.provided_value())
            .unwrap_or(320.0);
        let region = ScreenRegion::from_min_size(
            pos,
            Vector {
                x: width,
                y: height,
            },
        );

        // Per-column value formatters over the whole batch (handles every Arrow type).
        let opts = FormatOptions::default().with_display_error(true);
        let formatters: Vec<ArrayFormatter> = batch
            .columns()
            .iter()
            .map(|c| ArrayFormatter::try_new(c.as_ref(), &opts))
            .collect::<Result<_, _>>()
            .unwrap_or_default();

        const HEADER_H: f32 = 22.0;
        const ROW_H: f32 = 18.0;
        let salt = egui::Id::new(self.owner).with("table");

        let drawn = ctx.host_widgets(region, |ui| {
            // Make stripes visible.
            ui.visuals_mut().faint_bg_color = Color32::from_gray(240);

            // Size each column to its header text, clamped to a compact range.
            let heading_font = TextStyle::Heading.resolve(ui.style());
            let widths: Vec<f32> = schema
                .fields()
                .iter()
                .map(|field| {
                    let width = ui.fonts_mut(|fonts| {
                        fonts
                            .layout_no_wrap(
                                field.name().to_owned(),
                                heading_font.clone(),
                                Color32::PLACEHOLDER,
                            )
                            .rect
                            .width()
                    });
                    width.clamp(25.0, 100.0)
                })
                .collect();

            let mut builder = TableBuilder::new(ui)
                .id_salt(salt)
                .striped(self.striped)
                .auto_shrink([true, false])
                .cell_layout(Layout::left_to_right(Align::Center))
                .min_scrolled_height(0.0);
            for &width in &widths {
                builder = builder.column(Column::initial(width).at_least(25.0).resizable(true));
            }
            builder
                .header(HEADER_H, |mut header| {
                    for field in schema.fields() {
                        header.col(|ui| {
                            ui.strong(field.name());
                        });
                    }
                })
                .body(|body| {
                    body.rows(ROW_H, num_rows, |mut row| {
                        let r = row.index();
                        for fmt in &formatters {
                            row.col(|ui| {
                                ui.label(fmt.value(r).to_string());
                            });
                        }
                    });
                });
        });

        DrawResult::Complete {
            region: Some(drawn),
        }
    }
}

defhandlers! { TableWidget {} }
