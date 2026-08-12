use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use bt_core::events::{Bar, Timeframe};
use bt_core::types::Symbol;
use anyhow::Result;
use chrono::{DateTime, Datelike, TimeZone, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;

use arrow::array::{Array, Float64Array, Int32Array, Int64Array, RecordBatch};
use arrow::datatypes::{DataType, Field, Schema};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;

/// Partitioned Parquet storage for historical market bar data
/// Directory structure: base_dir/symbol/timeframe/YYYY-MM.parquet
pub struct ParquetStore {
    base_dir: PathBuf,
}

impl ParquetStore {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
        }
    }

    fn schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("timestamp_ms", DataType::Int64, false),
            Field::new("open", DataType::Float64, false),
            Field::new("high", DataType::Float64, false),
            Field::new("low", DataType::Float64, false),
            Field::new("close", DataType::Float64, false),
            Field::new("volume", DataType::Float64, false),
            Field::new("vwap", DataType::Float64, true),
            Field::new("trade_count", DataType::Int32, true),
        ]))
    }

    /// Write bars to partitioned Parquet files
    pub fn write_bars(&self, bars: &[Bar]) -> Result<usize> {
        if bars.is_empty() {
            return Ok(0);
        }

        // We assume all bars are for the same symbol and timeframe for grouping
        // Group by year and month
        use std::collections::HashMap;
        let mut grouped: HashMap<(Symbol, Timeframe, i32, u32), Vec<&Bar>> = HashMap::new();

        for bar in bars {
            let year = bar.timestamp.year();
            let month = bar.timestamp.month();
            grouped.entry((bar.symbol.clone(), bar.timeframe, year, month)).or_default().push(bar);
        }

        let schema = Self::schema();
        let mut total_written = 0;

        for ((symbol, timeframe, year, month), group_bars) in grouped {
            let symbol_str = symbol.ticker.replace("/", "_");
            let timeframe_str = format!("{:?}", timeframe).to_lowercase();
            let dir_path = self.base_dir.join(&symbol_str).join(&timeframe_str);

            fs::create_dir_all(&dir_path)?;

            let file_path = dir_path.join(format!("{:04}-{:02}.parquet", year, month));

            let mut timestamp_builder = Vec::with_capacity(group_bars.len());
            let mut open_builder = Vec::with_capacity(group_bars.len());
            let mut high_builder = Vec::with_capacity(group_bars.len());
            let mut low_builder = Vec::with_capacity(group_bars.len());
            let mut close_builder = Vec::with_capacity(group_bars.len());
            let mut volume_builder = Vec::with_capacity(group_bars.len());
            let mut vwap_builder = Vec::with_capacity(group_bars.len());
            let mut trade_count_builder = Vec::with_capacity(group_bars.len());

            for bar in &group_bars {
                timestamp_builder.push(bar.timestamp.timestamp_millis());
                open_builder.push(bar.open.to_f64().unwrap_or(0.0));
                high_builder.push(bar.high.to_f64().unwrap_or(0.0));
                low_builder.push(bar.low.to_f64().unwrap_or(0.0));
                close_builder.push(bar.close.to_f64().unwrap_or(0.0));
                volume_builder.push(bar.volume.to_f64().unwrap_or(0.0));
                vwap_builder.push(bar.vwap.and_then(|v| v.to_f64()));
                trade_count_builder.push(bar.trade_count.map(|v| v as i32));
            }

            let batch = RecordBatch::try_new(
                schema.clone(),
                vec![
                    Arc::new(Int64Array::from(timestamp_builder)),
                    Arc::new(Float64Array::from(open_builder)),
                    Arc::new(Float64Array::from(high_builder)),
                    Arc::new(Float64Array::from(low_builder)),
                    Arc::new(Float64Array::from(close_builder)),
                    Arc::new(Float64Array::from(volume_builder)),
                    Arc::new(Float64Array::from(vwap_builder)),
                    Arc::new(Int32Array::from(trade_count_builder)),
                ],
            )?;

            // In a real implementation we would merge with existing files.
            // For now, we just overwrite or create new.
            let file = fs::File::create(&file_path)?;
            let props = WriterProperties::builder().build();
            let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))?;
            
            writer.write(&batch)?;
            writer.close()?;

            total_written += group_bars.len();
        }

        Ok(total_written)
    }

    /// Read bars for a given symbol and timeframe within a time range
    pub fn read_bars(
        &self,
        symbol: &Symbol,
        timeframe: Timeframe,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Bar>> {
        let symbol_str = symbol.ticker.replace("/", "_");
        let timeframe_str = format!("{:?}", timeframe).to_lowercase();
        let dir_path = self.base_dir.join(&symbol_str).join(&timeframe_str);

        if !dir_path.exists() {
            return Ok(vec![]);
        }

        let mut start_date = start;
        let mut bars = Vec::new();
        let venue = bt_core::types::Venue::Simulator;

        while start_date <= end {
            let year = start_date.year();
            let month = start_date.month();
            let file_path = dir_path.join(format!("{:04}-{:02}.parquet", year, month));

            if file_path.exists() {
                let file = fs::File::open(&file_path)?;
                let builder = ParquetRecordBatchReaderBuilder::try_new(file)?;
                let reader = builder.build()?;

                for maybe_batch in reader {
                    let batch = maybe_batch?;
                    
                    let timestamps = batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap();
                    let opens = batch.column(1).as_any().downcast_ref::<Float64Array>().unwrap();
                    let highs = batch.column(2).as_any().downcast_ref::<Float64Array>().unwrap();
                    let lows = batch.column(3).as_any().downcast_ref::<Float64Array>().unwrap();
                    let closes = batch.column(4).as_any().downcast_ref::<Float64Array>().unwrap();
                    let volumes = batch.column(5).as_any().downcast_ref::<Float64Array>().unwrap();
                    let vwaps = batch.column(6).as_any().downcast_ref::<Float64Array>().unwrap();
                    let trade_counts = batch.column(7).as_any().downcast_ref::<Int32Array>().unwrap();

                    for i in 0..batch.num_rows() {
                        let ts_ms = timestamps.value(i);
                        let ts = Utc.timestamp_millis_opt(ts_ms).unwrap();

                        if ts >= start && ts <= end {
                            let bar = Bar {
                                symbol: symbol.clone(),
                                timeframe,
                                open: Decimal::from_f64(opens.value(i)).unwrap_or_default(),
                                high: Decimal::from_f64(highs.value(i)).unwrap_or_default(),
                                low: Decimal::from_f64(lows.value(i)).unwrap_or_default(),
                                close: Decimal::from_f64(closes.value(i)).unwrap_or_default(),
                                volume: Decimal::from_f64(volumes.value(i)).unwrap_or_default(),
                                vwap: if vwaps.is_null(i) { None } else { Decimal::from_f64(vwaps.value(i)) },
                                trade_count: if trade_counts.is_null(i) { None } else { Some(trade_counts.value(i) as u64) },
                                timestamp: ts,
                                venue,
                            };
                            bars.push(bar);
                        }
                    }
                }
            }

            // Move to next month
            if month == 12 {
                start_date = Utc.with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0).unwrap();
            } else {
                start_date = Utc.with_ymd_and_hms(year, month + 1, 1, 0, 0, 0).unwrap();
            }
        }

        Ok(bars)
    }

    /// List all stored symbols
    pub fn list_symbols(&self) -> Result<Vec<String>> {
        let mut symbols = Vec::new();
        if !self.base_dir.exists() {
            return Ok(symbols);
        }
        
        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    symbols.push(name.replace("_", "/"));
                }
            }
        }
        
        Ok(symbols)
    }

    /// Get storage statistics
    pub fn stats(&self) -> Result<StoreStats> {
        let mut total_files = 0;
        let mut total_bytes = 0;
        let mut symbols = 0;

        if !self.base_dir.exists() {
            return Ok(StoreStats { total_files, total_bytes, symbols });
        }

        for symbol_entry in fs::read_dir(&self.base_dir)? {
            let symbol_entry = symbol_entry?;
            if symbol_entry.file_type()?.is_dir() {
                symbols += 1;
                for tf_entry in fs::read_dir(symbol_entry.path())? {
                    let tf_entry = tf_entry?;
                    if tf_entry.file_type()?.is_dir() {
                        for file_entry in fs::read_dir(tf_entry.path())? {
                            let file_entry = file_entry?;
                            if file_entry.file_type()?.is_file() {
                                total_files += 1;
                                total_bytes += file_entry.metadata()?.len();
                            }
                        }
                    }
                }
            }
        }

        Ok(StoreStats {
            total_files,
            total_bytes,
            symbols,
        })
    }
}

#[derive(Debug, Clone)]
pub struct StoreStats {
    pub total_files: usize,
    pub total_bytes: u64,
    pub symbols: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_parquet_roundtrip() {
        let dir = tempdir().unwrap();
        let store = ParquetStore::new(dir.path());
        let symbol = Symbol::parse("BTC/USD").unwrap();

        let bar1 = Bar {
            symbol: symbol.clone(),
            timeframe: Timeframe::Minute,
            open: Decimal::new(100, 0),
            high: Decimal::new(110, 0),
            low: Decimal::new(90, 0),
            close: Decimal::new(105, 0),
            volume: Decimal::new(1000, 0),
            vwap: Some(Decimal::new(101, 0)),
            trade_count: Some(50),
            timestamp: Utc.with_ymd_and_hms(2023, 1, 15, 12, 0, 0).unwrap(),
            venue: bt_core::types::Venue::Simulator,
        };

        let bar2 = Bar {
            symbol: symbol.clone(),
            timeframe: Timeframe::Minute,
            open: Decimal::new(105, 0),
            high: Decimal::new(115, 0),
            low: Decimal::new(95, 0),
            close: Decimal::new(110, 0),
            volume: Decimal::new(2000, 0),
            vwap: None,
            trade_count: None,
            timestamp: Utc.with_ymd_and_hms(2023, 1, 15, 12, 1, 0).unwrap(),
            venue: bt_core::types::Venue::Simulator,
        };

        let bars = vec![bar1, bar2];
        store.write_bars(&bars).unwrap();

        let start = Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2023, 1, 31, 23, 59, 59).unwrap();

        let read = store.read_bars(&symbol, Timeframe::Minute, start, end).unwrap();
        assert_eq!(read.len(), 2);
        
        assert_eq!(read[0].open, Decimal::new(100, 0));
        assert_eq!(read[0].trade_count, Some(50));
        assert_eq!(read[1].vwap, None);

        let stats = store.stats().unwrap();
        assert_eq!(stats.symbols, 1);
        assert_eq!(stats.total_files, 1);
        assert!(stats.total_bytes > 0);

        let symbols = store.list_symbols().unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0], "BTC/USD");
    }
}
