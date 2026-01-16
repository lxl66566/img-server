use std::path::PathBuf;

use flexi_logger::{
    Age, Cleanup, Criterion, DeferredNow, Duplicate, FileSpec, Logger, LoggerHandle, Naming,
    Record, WriteMode,
};

pub struct LoggerGuard(LoggerHandle);

impl LoggerGuard {
    pub fn new(dir: PathBuf) -> Self {
        let handle = init_logger(dir).unwrap();
        Self(handle)
    }
}

impl Drop for LoggerGuard {
    fn drop(&mut self) {
        self.0.flush();
        self.0.shutdown();
    }
}

fn my_log_format(
    w: &mut dyn std::io::Write,
    now: &mut DeferredNow,
    record: &Record,
) -> std::io::Result<()> {
    write!(
        w,
        "[{time}] {level} - {message}",
        time = now.format("%Y-%m-%d %H:%M:%S"), // 时间
        level = record.level(),                 // 等级
        message = record.args()                 // 日志内容
    )
}

pub fn init_logger(dir: PathBuf) -> Result<LoggerHandle, flexi_logger::FlexiLoggerError> {
    let handle = Logger::try_with_env_or_str("info")?
        .log_to_file(FileSpec::default().directory(dir).suppress_basename())
        .rotate(
            Criterion::Age(Age::Day),
            Naming::Timestamps,
            Cleanup::KeepLogAndCompressedFiles(5, 30),
        )
        .format(my_log_format)
        .duplicate_to_stderr(Duplicate::All)
        .write_mode(WriteMode::BufferAndFlush)
        .start()?;
    Ok(handle)
}
