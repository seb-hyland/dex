use arrow::{
    array::RecordBatch,
    compute::concat_batches,
    csv::{ReaderBuilder, reader::Format},
};
use std::{
    io::{Read, Seek},
    path::Path,
    sync::Arc,
};

pub fn sniff_file<R: Read + Seek>(mut file: R, path: &Path) -> Result<Format, String> {
    #[derive(Default)]
    struct CsvSniff {
        quote: Option<u8>,
        comment: Option<u8>,
        delim: Option<u8>,
        has_header: Option<bool>,
    }

    let mut sniff = CsvSniff::default();
    let mut buf = [0b0; 25000];
    let bytes_read = file
        .read(&mut buf)
        .map_err(|e| format!("Failed to read file: {e}"))?;
    let buf = &buf[0..bytes_read];

    // If CSV/TSV, assume delim
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("csv") => sniff.delim = Some(b','),
        Some("tsv") => sniff.delim = Some(b'\t'),
        _ => {},
    };

    // Determine quote char
    let quotes = [b'\'', b'"'];
    let mut quote_counts = [0u32; 2];
    for &b in buf {
        if b == quotes[0] {
            quote_counts[0] += 1;
        } else if b == quotes[1] {
            quote_counts[1] += 1;
        }
    }
    sniff.quote = if quote_counts[0] > quote_counts[1] {
        Some(quotes[0])
    } else if quote_counts == [0, 0] {
        None
    } else {
        Some(quotes[1])
    };

    // Determine comment char
    let comments = [b'#', b'/', b';', b'-', b'%', b'!'];
    let mut comment_counts = [0u32; 6];
    let mut at_line_start = true;
    for &b in buf {
        if at_line_start {
            'inner: for (comment, comment_count) in
                comments.into_iter().zip(comment_counts.iter_mut())
            {
                if comment == b {
                    *comment_count += 1;
                    break 'inner;
                }
            }
            at_line_start = false;
        } else if b == b'\n' {
            at_line_start = true;
        }
    }
    sniff.comment = if comment_counts == [0, 0, 0, 0, 0, 0] {
        None
    } else {
        let max_idx = comment_counts
            .into_iter()
            .enumerate()
            .max_by(|(_, val1), (_, val2)| u32::cmp(val1, val2))
            .unwrap()
            .0;
        Some(comments[max_idx])
    };

    // Find delim if not already found
    if let None = sniff.delim {
        let mut inside_quotes = false;
        let mut inside_skip_line = false;
        let mut at_line_start = true;
        let delim = [b',', b';', b'\t', b'|', b':'];
        let mut delim_counts = [0u32; 5];

        'buf_loop: for &b in buf {
            if Some(b) == sniff.quote {
                inside_quotes = !inside_quotes;
                continue 'buf_loop;
            }
            if b == b'\n' {
                at_line_start = true;
                inside_skip_line = false;
                continue 'buf_loop;
            }
            if at_line_start {
                if Some(b) == sniff.comment {
                    inside_skip_line = true;
                }
                at_line_start = false;
            }
            if inside_quotes || inside_skip_line {
                continue 'buf_loop;
            }

            'inner: for (delim, delim_count) in delim.into_iter().zip(delim_counts.iter_mut()) {
                if b == delim {
                    *delim_count += 1;
                    break 'inner;
                }
            }
        }

        let max_idx = delim_counts
            .into_iter()
            .enumerate()
            .max_by(|(_, val1), (_, val2)| u32::cmp(val1, val2))
            .unwrap()
            .0;
        sniff.delim = Some(delim[max_idx]);
    }

    // Determine if table has header
    let mut inside_skip_line = false;
    let mut at_line_start = true;
    let mut line_num = 0;
    let mut header_test = Vec::new();
    let mut second_test = Vec::new();
    'buf_loop: for &b in buf {
        if b == b'\n' {
            if inside_skip_line {
                at_line_start = true;
                inside_skip_line = false;
                continue 'buf_loop;
            }
            if line_num == 0 {
                line_num = 1;
                continue 'buf_loop;
            }
            if line_num == 1 {
                break 'buf_loop;
            }
        }
        if at_line_start {
            if Some(b) == sniff.comment {
                inside_skip_line = true;
                continue 'buf_loop;
            }
            at_line_start = false;
        }
        if inside_skip_line {
            continue 'buf_loop;
        }
        if line_num == 0 {
            header_test.push(b);
        } else if line_num == 1 {
            second_test.push(b);
        }
    }
    sniff.has_header = if header_test.is_empty() || second_test.is_empty() {
        Some(false)
    } else {
        let collect_parts = |buf: Vec<u8>| -> Vec<Vec<u8>> {
            let mut inside_quotes = false;
            let mut res = Vec::new();
            let mut cur_start = 0;
            let mut cur_end = 0;
            for (i, b) in buf.iter().enumerate() {
                if i == buf.len() - 1 {
                    cur_end = i + 1;
                    res.push(Vec::from(&buf[cur_start..cur_end]));
                }
                if Some(b) == sniff.quote.as_ref() {
                    inside_quotes = !inside_quotes;
                    continue;
                }
                if inside_quotes {
                    continue;
                }
                if *b == sniff.delim.unwrap() {
                    cur_end = i;
                    res.push(Vec::from(&buf[cur_start..cur_end]));
                    cur_start = i + 1;
                }
            }
            res
        };
        let header_parts = collect_parts(header_test);
        let second_parts = collect_parts(second_test);

        #[derive(PartialEq)]
        enum CsvType {
            Bool,
            Int,
            Float,
            String,
        }
        fn to_type(buf: Vec<u8>) -> CsvType {
            if buf == b"true" || buf == b"false" {
                return CsvType::Bool;
            }
            let s = String::from_utf8_lossy(&buf);
            if s.parse::<i32>().is_ok() {
                CsvType::Int
            } else if s.parse::<f64>().is_ok() {
                CsvType::Float
            } else {
                CsvType::String
            }
        }
        let header_types: Vec<_> = header_parts.into_iter().map(to_type).collect();
        let second_types: Vec<_> = second_parts.into_iter().map(to_type).collect();
        if header_types != second_types {
            Some(true)
        } else {
            Some(false)
        }
    };

    let mut csv_format = Format::default()
        .with_delimiter(sniff.delim.unwrap())
        .with_header(sniff.has_header.unwrap());
    if let Some(q) = sniff.quote {
        csv_format = csv_format.with_quote(q);
    }
    if let Some(c) = sniff.comment {
        csv_format = csv_format.with_comment(c);
    }

    Ok(csv_format)
}

pub fn open_csv<R: Read>(
    schema_file: R,
    read_file: R,
    format: Format,
) -> Result<RecordBatch, String> {
    let (schema, _len) = format
        .infer_schema(schema_file, Some(100))
        .map_err(|e| format!("Failed to infer structure of CSV file: {e}"))?;
    let schema_arc = Arc::new(schema);

    let reader = ReaderBuilder::new(schema_arc.clone())
        .with_format(format)
        .build(read_file)
        .map_err(|e| format!("Failed to read CSV file: {e}"))?;
    let batches = reader
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to read CSV file: {e}"))?;
    concat_batches(&schema_arc, batches.iter())
        .map_err(|e| format!("Failed to complete CSV read: {e}"))
}
