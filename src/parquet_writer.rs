//! Arrow/Parquet persistence for passive Thornwave observations.
//!
//! The writer batches [`Sample`] values into Arrow [`RecordBatch`] values and
//! writes them to Parquet files. Active files use a `.parquet.inprogress` suffix
//! and are renamed to `.parquet` only after the Parquet footer has been written
//! successfully by closing the [`ArrowWriter`].

use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use arrow::{
    array::{
        ArrayRef, BooleanArray, Float32Array, Int16Array, Int32Array, Int64Array, StringArray,
        TimestampMillisecondArray, UInt8Array, UInt16Array, UInt32Array, UInt64Array,
    },
    datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit},
    record_batch::RecordBatch,
};
use chrono::{DateTime, Datelike, Timelike, Utc};
use parquet::{
    arrow::ArrowWriter,
    basic::{Compression, ZstdLevel},
    file::properties::WriterProperties,
};
use tokio::{
    sync::mpsc,
    time::{self, Duration},
};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::{config::RollPeriod, metrics::Metrics, sample::Sample};

const BATCH_ROWS: usize = 4096;
const FLUSH_INTERVAL: Duration = Duration::from_secs(10);

/// Runtime configuration for the Parquet writer task.
///
/// The public CLI intentionally exposes only the data directory and file roll
/// period. Batch sizing and flush interval are fixed constants for v1.
#[derive(Clone, Debug)]
pub struct ParquetWriterConfig {
    data_dir: PathBuf,
    roll: RollPeriod,
}

impl ParquetWriterConfig {
    /// Creates writer configuration from the selected data directory and roll period.
    #[must_use]
    pub fn new(data_dir: PathBuf, roll: RollPeriod) -> Self {
        Self { data_dir, roll }
    }
}

#[derive(Debug)]
struct ActiveWriter {
    window_start: DateTime<Utc>,
    inprogress_path: PathBuf,
    complete_path: PathBuf,
    schema: SchemaRef,
    rows: u64,
    writer: Option<ArrowWriter<File>>,
}

/// Runs the Parquet writer task until input closes or shutdown is requested.
///
/// The task buffers samples, flushes on row count or time interval, rolls files
/// at UTC hour/day boundaries, and finalizes the active file during graceful
/// shutdown. Write failures are logged and counted but do not block scanner
/// processing.
///
/// # Errors
///
/// Returns an error if file creation, rolling, writing, or finalization fails.
pub async fn run_writer(
    config: ParquetWriterConfig,
    mut rx: mpsc::Receiver<Sample>,
    metrics: Arc<Metrics>,
    shutdown: CancellationToken,
) -> Result<()> {
    let schema = arrow_schema();
    let mut active: Option<ActiveWriter> = None;
    let mut buffer = Vec::with_capacity(BATCH_ROWS);
    let mut flush_interval = time::interval(FLUSH_INTERVAL);

    loop {
        tokio::select! {
            biased;
            Some(sample) = rx.recv() => {
                maybe_roll(&config, &schema, &mut active, &mut buffer, &metrics)?;
                ensure_active(&config, &schema, &mut active, &metrics)?;
                buffer.push(sample);
                if buffer.len() >= BATCH_ROWS {
                    flush_buffer(&mut active, &mut buffer, &metrics);
                }
            }
            _ = flush_interval.tick() => {
                maybe_roll(&config, &schema, &mut active, &mut buffer, &metrics)?;
                flush_buffer(&mut active, &mut buffer, &metrics);
            }
            () = shutdown.cancelled() => break,
            else => break,
        }
    }

    while let Ok(sample) = rx.try_recv() {
        maybe_roll(&config, &schema, &mut active, &mut buffer, &metrics)?;
        ensure_active(&config, &schema, &mut active, &metrics)?;
        buffer.push(sample);
        if buffer.len() >= BATCH_ROWS {
            flush_buffer(&mut active, &mut buffer, &metrics);
        }
    }

    flush_buffer(&mut active, &mut buffer, &metrics);
    if let Some(writer) = active.take() {
        close_active(writer, &metrics)?;
    }

    Ok(())
}

/// Returns the Arrow schema used for Parquet sample files.
///
/// This schema is versioned by the `schema_version` column and should remain
/// stable for v1 so downstream readers can rely on fixed column names and types.
#[must_use]
pub fn arrow_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("schema_version", DataType::Int32, false),
        Field::new("observed_at_unix_ms", DataType::Int64, false),
        Field::new(
            "observed_at_utc",
            DataType::Timestamp(TimeUnit::Millisecond, Some("UTC".into())),
            false,
        ),
        Field::new("serial", DataType::UInt64, false),
        Field::new("address_raw", DataType::UInt64, false),
        Field::new("address_display", DataType::Utf8, true),
        Field::new("address_kind", DataType::Utf8, true),
        Field::new("name", DataType::Utf8, true),
        Field::new("model", DataType::Utf8, true),
        Field::new("firmware_version_bcd", DataType::UInt16, false),
        Field::new("firmware_version", DataType::Utf8, true),
        Field::new("hardware_revision_bcd", DataType::UInt8, false),
        Field::new("hardware_revision", DataType::Utf8, true),
        Field::new("device_time_raw", DataType::UInt32, false),
        Field::new("flags_raw", DataType::UInt32, false),
        Field::new("voltage1_volts", DataType::Float32, false),
        Field::new("voltage2_volts", DataType::Float32, false),
        Field::new("current_amps", DataType::Float32, false),
        Field::new("power_watts", DataType::Float32, false),
        Field::new("coulomb_meter_raw", DataType::Float32, false),
        Field::new("power_meter_raw", DataType::Float32, false),
        Field::new("temperature_celsius", DataType::Float32, false),
        Field::new("temperature_is_external", DataType::Boolean, false),
        Field::new("power_status_code", DataType::UInt8, false),
        Field::new("power_status", DataType::Utf8, true),
        Field::new("soc_raw", DataType::UInt8, false),
        Field::new("soc_percent", DataType::UInt8, true),
        Field::new("runtime_raw", DataType::UInt16, false),
        Field::new("runtime_minutes", DataType::UInt16, true),
        Field::new("rssi_dbm", DataType::Int16, false),
    ]))
}

/// Converts a slice of samples into an Arrow record batch with the provided schema.
///
/// Nullable derived fields are represented with Arrow nulls while raw fields are
/// always populated. The caller is responsible for passing [`arrow_schema`] or a
/// compatible schema reference.
///
/// # Errors
///
/// Returns an error if Arrow rejects the arrays for the provided schema.
#[expect(
    clippy::too_many_lines,
    reason = "schema-to-array mapping is deliberately kept linear and column-ordered"
)]
pub fn samples_to_record_batch(samples: &[Sample], schema: SchemaRef) -> Result<RecordBatch> {
    let observed_at_ms: Vec<i64> = samples.iter().map(Sample::observed_at_unix_ms).collect();

    let arrays: Vec<ArrayRef> = vec![
        Arc::new(Int32Array::from_iter_values(
            samples.iter().map(|sample| sample.schema_version),
        )),
        Arc::new(Int64Array::from_iter_values(observed_at_ms.iter().copied())),
        Arc::new(TimestampMillisecondArray::from(observed_at_ms).with_timezone("UTC")),
        Arc::new(UInt64Array::from_iter_values(
            samples.iter().map(|sample| sample.serial),
        )),
        Arc::new(UInt64Array::from_iter_values(
            samples.iter().map(|sample| sample.address_raw),
        )),
        Arc::new(StringArray::from(
            samples
                .iter()
                .map(|sample| sample.address_display.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            samples
                .iter()
                .map(|sample| sample.address_kind.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            samples
                .iter()
                .map(|sample| sample.name.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            samples
                .iter()
                .map(|sample| sample.model.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(UInt16Array::from_iter_values(
            samples.iter().map(|sample| sample.firmware_version_bcd),
        )),
        Arc::new(StringArray::from(
            samples
                .iter()
                .map(|sample| sample.firmware_version.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(UInt8Array::from_iter_values(
            samples.iter().map(|sample| sample.hardware_revision_bcd),
        )),
        Arc::new(StringArray::from(
            samples
                .iter()
                .map(|sample| sample.hardware_revision.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(UInt32Array::from_iter_values(
            samples.iter().map(|sample| sample.device_time_raw),
        )),
        Arc::new(UInt32Array::from_iter_values(
            samples.iter().map(|sample| sample.flags_raw),
        )),
        Arc::new(Float32Array::from_iter_values(
            samples.iter().map(|sample| sample.voltage1_volts),
        )),
        Arc::new(Float32Array::from_iter_values(
            samples.iter().map(|sample| sample.voltage2_volts),
        )),
        Arc::new(Float32Array::from_iter_values(
            samples.iter().map(|sample| sample.current_amps),
        )),
        Arc::new(Float32Array::from_iter_values(
            samples.iter().map(|sample| sample.power_watts),
        )),
        Arc::new(Float32Array::from_iter_values(
            samples.iter().map(|sample| sample.coulomb_meter_raw),
        )),
        Arc::new(Float32Array::from_iter_values(
            samples.iter().map(|sample| sample.power_meter_raw),
        )),
        Arc::new(Float32Array::from_iter_values(
            samples.iter().map(|sample| sample.temperature_celsius),
        )),
        Arc::new(
            samples
                .iter()
                .map(|sample| sample.temperature_is_external)
                .collect::<BooleanArray>(),
        ),
        Arc::new(UInt8Array::from_iter_values(
            samples.iter().map(|sample| sample.power_status_code),
        )),
        Arc::new(StringArray::from(
            samples
                .iter()
                .map(|sample| sample.power_status.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(UInt8Array::from_iter_values(
            samples.iter().map(|sample| sample.soc_raw),
        )),
        Arc::new(UInt8Array::from(
            samples
                .iter()
                .map(|sample| sample.soc_percent)
                .collect::<Vec<_>>(),
        )),
        Arc::new(UInt16Array::from_iter_values(
            samples.iter().map(|sample| sample.runtime_raw),
        )),
        Arc::new(UInt16Array::from(
            samples
                .iter()
                .map(|sample| sample.runtime_minutes)
                .collect::<Vec<_>>(),
        )),
        Arc::new(Int16Array::from_iter_values(
            samples.iter().map(|sample| sample.rssi_dbm),
        )),
    ];

    RecordBatch::try_new(schema, arrays).context("failed to build Arrow record batch")
}

fn maybe_roll(
    config: &ParquetWriterConfig,
    schema: &SchemaRef,
    active: &mut Option<ActiveWriter>,
    buffer: &mut Vec<Sample>,
    metrics: &Metrics,
) -> Result<()> {
    let Some(current) = active.as_ref() else {
        return Ok(());
    };

    let current_window = roll_window_start(Utc::now(), config.roll);
    if current.window_start == current_window {
        return Ok(());
    }

    flush_buffer(active, buffer, metrics);
    if let Some(writer) = active.take() {
        close_active(writer, metrics)?;
    }
    ensure_active(config, schema, active, metrics)
}

fn ensure_active(
    config: &ParquetWriterConfig,
    schema: &SchemaRef,
    active: &mut Option<ActiveWriter>,
    metrics: &Metrics,
) -> Result<()> {
    if active.is_none() {
        *active = Some(open_active(config, Arc::clone(schema), metrics)?);
    }
    Ok(())
}

fn open_active(
    config: &ParquetWriterConfig,
    schema: SchemaRef,
    metrics: &Metrics,
) -> Result<ActiveWriter> {
    let window_start = roll_window_start(Utc::now(), config.roll);
    let directory = partition_directory(&config.data_dir, config.roll, window_start);
    std::fs::create_dir_all(&directory).with_context(|| {
        format!(
            "failed to create Parquet partition directory {}",
            directory.display()
        )
    })?;

    let (inprogress_path, complete_path) =
        unique_file_paths(&directory, config.roll, window_start)?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&inprogress_path)
        .with_context(|| {
            format!(
                "failed to create active Parquet file {}",
                inprogress_path.display()
            )
        })?;

    let properties = WriterProperties::builder()
        .set_compression(Compression::ZSTD(ZstdLevel::default()))
        .build();
    let writer =
        ArrowWriter::try_new(file, Arc::clone(&schema), Some(properties)).with_context(|| {
            format!(
                "failed to open Parquet writer for {}",
                inprogress_path.display()
            )
        })?;

    metrics.set_parquet_active_file_open(true);
    metrics.set_parquet_active_file_rows(0);
    info!(path = %inprogress_path.display(), "opened active Parquet file");

    Ok(ActiveWriter {
        window_start,
        inprogress_path,
        complete_path,
        schema,
        rows: 0,
        writer: Some(writer),
    })
}

fn flush_buffer(active: &mut Option<ActiveWriter>, buffer: &mut Vec<Sample>, metrics: &Metrics) {
    if buffer.is_empty() {
        return;
    }

    let Some(writer) = active.as_mut() else {
        return;
    };

    let schema = Arc::clone(&writer.schema);
    match samples_to_record_batch(buffer, schema) {
        Ok(batch) => {
            let rows = batch.num_rows() as u64;
            let write_result = writer
                .writer
                .as_mut()
                .expect("writer must be present while active")
                .write(&batch);
            match write_result {
                Ok(()) => {
                    writer.rows = writer.rows.saturating_add(rows);
                    metrics.parquet_rows_written(rows, writer.rows);
                    buffer.clear();
                }
                Err(error) => {
                    metrics.parquet_write_error();
                    warn!(?error, path = %writer.inprogress_path.display(), "failed to write Parquet batch; dropping buffered rows");
                    buffer.clear();
                }
            }
        }
        Err(error) => {
            metrics.parquet_write_error();
            warn!(
                ?error,
                "failed to convert samples to Arrow batch; dropping buffered rows"
            );
            buffer.clear();
        }
    }
}

fn close_active(mut active: ActiveWriter, metrics: &Metrics) -> Result<()> {
    if let Some(writer) = active.writer.take() {
        writer.close().with_context(|| {
            format!(
                "failed to close Parquet writer {}",
                active.inprogress_path.display()
            )
        })?;
    }

    std::fs::rename(&active.inprogress_path, &active.complete_path).with_context(|| {
        format!(
            "failed to rename completed Parquet file {} to {}",
            active.inprogress_path.display(),
            active.complete_path.display()
        )
    })?;

    metrics.set_parquet_active_file_open(false);
    metrics.set_parquet_active_file_rows(0);
    metrics.parquet_roll();
    info!(path = %active.complete_path.display(), rows = active.rows, "completed Parquet file");
    Ok(())
}

fn roll_window_start(now: DateTime<Utc>, roll: RollPeriod) -> DateTime<Utc> {
    match roll {
        RollPeriod::Hourly => now
            .with_minute(0)
            .and_then(|value| value.with_second(0))
            .and_then(|value| value.with_nanosecond(0))
            .expect("valid hour window"),
        RollPeriod::Daily => now
            .with_hour(0)
            .and_then(|value| value.with_minute(0))
            .and_then(|value| value.with_second(0))
            .and_then(|value| value.with_nanosecond(0))
            .expect("valid day window"),
    }
}

fn partition_directory(data_dir: &Path, roll: RollPeriod, window_start: DateTime<Utc>) -> PathBuf {
    let date = format!(
        "date={:04}-{:02}-{:02}",
        window_start.year(),
        window_start.month(),
        window_start.day()
    );
    match roll {
        RollPeriod::Hourly => data_dir
            .join("samples")
            .join(date)
            .join(format!("hour={:02}", window_start.hour())),
        RollPeriod::Daily => data_dir.join("samples").join(date),
    }
}

fn unique_file_paths(
    directory: &Path,
    roll: RollPeriod,
    window_start: DateTime<Utc>,
) -> Result<(PathBuf, PathBuf)> {
    let stem = match roll {
        RollPeriod::Hourly => format!("part-{}", window_start.format("%Y%m%dT%H0000Z")),
        RollPeriod::Daily => format!("part-{}", window_start.format("%Y%m%d")),
    };

    for index in 0..1000 {
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("-{index:03}")
        };
        let complete = directory.join(format!("{stem}{suffix}.parquet"));
        let inprogress = directory.join(format!("{stem}{suffix}.parquet.inprogress"));
        if !complete.exists() && !inprogress.exists() {
            return Ok((inprogress, complete));
        }
    }

    anyhow::bail!(
        "could not find an unused Parquet filename in {}",
        directory.display()
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::TimeZone;

    use super::{arrow_schema, partition_directory, roll_window_start, samples_to_record_batch};
    use crate::{config::RollPeriod, sample::Sample};

    #[test]
    fn creates_hourly_partition_paths() {
        let start = roll_window_start(
            chrono::Utc
                .with_ymd_and_hms(2026, 6, 21, 14, 23, 59)
                .unwrap(),
            RollPeriod::Hourly,
        );
        let path = partition_directory(std::path::Path::new("/data"), RollPeriod::Hourly, start);
        assert_eq!(
            path,
            std::path::Path::new("/data/samples/date=2026-06-21/hour=14")
        );
    }

    #[test]
    fn creates_daily_partition_paths() {
        let start = roll_window_start(
            chrono::Utc
                .with_ymd_and_hms(2026, 6, 21, 14, 23, 59)
                .unwrap(),
            RollPeriod::Daily,
        );
        let path = partition_directory(std::path::Path::new("/data"), RollPeriod::Daily, start);
        assert_eq!(path, std::path::Path::new("/data/samples/date=2026-06-21"));
    }

    #[test]
    fn sample_record_batch_matches_timestamp_schema() {
        let schema = arrow_schema();
        let batch = samples_to_record_batch(&[sample()], Arc::clone(&schema))
            .expect("sample should convert to a record batch");

        assert_eq!(
            batch.schema().field(2).data_type(),
            schema.field(2).data_type()
        );
    }

    fn sample() -> Sample {
        Sample {
            schema_version: 1,
            observed_at: chrono::Utc
                .with_ymd_and_hms(2026, 6, 22, 14, 55, 0)
                .unwrap(),
            serial: 239_148_418_773_806,
            address_raw: 0xAABB_CCDD_EEFF,
            address_display: Some("AA:BB:CC:DD:EE:FF".to_string()),
            address_kind: Some("ble_mac".to_string()),
            name: Some("PowerMon".to_string()),
            model: Some("PowerMon".to_string()),
            firmware_version_bcd: 0x0120,
            firmware_version: Some("01.20".to_string()),
            hardware_revision_bcd: 0x20,
            hardware_revision: Some("2.0".to_string()),
            device_time_raw: 0,
            flags_raw: 0,
            voltage1_volts: 13.2,
            voltage2_volts: 0.0,
            current_amps: 1.5,
            power_watts: 19.8,
            coulomb_meter_raw: 0.0,
            power_meter_raw: 0.0,
            temperature_celsius: 25.0,
            temperature_is_external: false,
            power_status_code: 1,
            power_status: Some("On".to_string()),
            soc_raw: 80,
            soc_percent: Some(80),
            runtime_raw: 120,
            runtime_minutes: Some(120),
            rssi_dbm: -60,
        }
    }
}
