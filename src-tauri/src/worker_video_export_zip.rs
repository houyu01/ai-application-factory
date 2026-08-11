//! Minimal UTF-8 ZIP writer for generated episode files, avoiding a second archive runtime dependency.

use std::{
    fs::File,
    io::{self, Read, Seek, Write},
    path::{Path, PathBuf},
};

use crate::error::{AppError, AppResult};

struct ZipEntry {
    crc: u32,
    name: String,
    offset: u32,
    size: u32,
}

/// Write stored (uncompressed) video files into a standards-compliant UTF-8 ZIP archive.
///
/// MP4 content is already compressed, so storing instead of recompressing keeps the local export flow
/// predictable without adding an archive crate or spending creator time on redundant work.
pub(super) fn write_zip(destination: &Path, files: &[(String, PathBuf)]) -> AppResult<()> {
    let mut output = File::create(destination)?;
    let mut entries = Vec::with_capacity(files.len());
    for (name, path) in files {
        let offset = u32_size(output.stream_position()?, "ZIP 文件过大")?;
        let (crc, size) = crc32_file(path)?;
        let name_bytes = name.as_bytes();
        write_u32(&mut output, 0x0403_4b50)?;
        write_u16(&mut output, 20)?;
        write_u16(&mut output, 0x0800)?;
        write_u16(&mut output, 0)?;
        write_u16(&mut output, 0)?;
        write_u16(&mut output, 0)?;
        write_u32(&mut output, crc)?;
        write_u32(&mut output, size)?;
        write_u32(&mut output, size)?;
        write_u16(&mut output, u16_size(name_bytes.len(), "ZIP 条目名称过长")?)?;
        write_u16(&mut output, 0)?;
        output.write_all(name_bytes)?;
        io::copy(&mut File::open(path)?, &mut output)?;
        entries.push(ZipEntry {
            crc,
            name: name.clone(),
            offset,
            size,
        });
    }
    let directory_start = u32_size(output.stream_position()?, "ZIP 文件过大")?;
    for entry in &entries {
        let name = entry.name.as_bytes();
        write_u32(&mut output, 0x0201_4b50)?;
        write_u16(&mut output, 20)?;
        write_u16(&mut output, 20)?;
        write_u16(&mut output, 0x0800)?;
        write_u16(&mut output, 0)?;
        write_u16(&mut output, 0)?;
        write_u16(&mut output, 0)?;
        write_u32(&mut output, entry.crc)?;
        write_u32(&mut output, entry.size)?;
        write_u32(&mut output, entry.size)?;
        write_u16(&mut output, u16_size(name.len(), "ZIP 条目名称过长")?)?;
        write_u16(&mut output, 0)?;
        write_u16(&mut output, 0)?;
        write_u16(&mut output, 0)?;
        write_u16(&mut output, 0)?;
        write_u32(&mut output, 0)?;
        write_u32(&mut output, entry.offset)?;
        output.write_all(name)?;
    }
    let directory_size = u32_size(
        output.stream_position()? - u64::from(directory_start),
        "ZIP 文件过大",
    )?;
    let count = u16_size(entries.len(), "ZIP 包含过多剧集")?;
    write_u32(&mut output, 0x0605_4b50)?;
    write_u16(&mut output, 0)?;
    write_u16(&mut output, 0)?;
    write_u16(&mut output, count)?;
    write_u16(&mut output, count)?;
    write_u32(&mut output, directory_size)?;
    write_u32(&mut output, directory_start)?;
    write_u16(&mut output, 0)?;
    Ok(())
}

fn crc32_file(path: &Path) -> AppResult<(u32, u32)> {
    let mut file = File::open(path)?;
    let mut buffer = [0_u8; 32 * 1024];
    let mut crc = !0_u32;
    let mut size = 0_u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size += read as u64;
        for byte in &buffer[..read] {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
            }
        }
    }
    Ok((!crc, u32_size(size, "单集视频超过 ZIP 格式上限")?))
}

fn u16_size(value: usize, message: &str) -> AppResult<u16> {
    u16::try_from(value).map_err(|_| AppError::BadRequest(message.to_owned()))
}

fn u32_size(value: u64, message: &str) -> AppResult<u32> {
    u32::try_from(value).map_err(|_| AppError::BadRequest(message.to_owned()))
}

fn write_u16(output: &mut File, value: u16) -> io::Result<()> {
    output.write_all(&value.to_le_bytes())
}
fn write_u32(output: &mut File, value: u32) -> io::Result<()> {
    output.write_all(&value.to_le_bytes())
}

#[cfg(test)]
mod tests {
    use super::{crc32_file, write_zip};
    use crate::value::new_id;

    #[test]
    fn crc32_matches_the_zip_standard_vector() {
        let path = std::env::temp_dir().join(format!("ai-video-export-{}.txt", new_id()));
        std::fs::write(&path, b"123456789").expect("write vector");
        assert_eq!(crc32_file(&path).expect("crc").0, 0xcbf4_3926);
        std::fs::remove_file(path).expect("remove vector");
    }

    #[test]
    fn writes_a_zip_that_the_system_reader_can_open() {
        let root = std::env::temp_dir().join(format!("ai-video-export-{}", new_id()));
        std::fs::create_dir_all(&root).expect("create root");
        let entry = root.join("第001集.mp4");
        std::fs::write(&entry, b"sample video bytes").expect("write entry");
        let archive = root.join("archive.zip");
        write_zip(&archive, &[("第001集.mp4".to_owned(), entry)]).expect("write zip");
        assert!(std::process::Command::new("unzip")
            .args(["-t", archive.to_str().expect("path")])
            .status()
            .expect("run unzip")
            .success());
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
