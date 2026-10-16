//! Validator for the `.dds` (picture) files that are used in the game.

use std::fs::{File, metadata};
use std::io::{Read, Result};
use std::path::PathBuf;

use crate::fileset::{FileEntry, FileHandler};
use crate::helpers::TigerHashMap;
use crate::parse::ParserMemory;
use crate::report::{ErrorKey, err, tips, warn};
use crate::token::Token;

const DDS_HEADER_SIZE: usize = 124;
const DDS_HEIGHT_OFFSET: usize = 12;
const DDS_WIDTH_OFFSET: usize = 16;
const DDS_PIXELFORMAT_FLAGS_OFFSET: usize = 80;
const DDS_PIXELFORMAT_FOURCC_OFFSET: usize = 84;
const DDS_PIXELFORMAT_BITCOUNT_OFFSET: usize = 88;
const DDS_PIXELFORMAT_MASKS_OFFSET: usize = 92;

fn from_le32(buffer: &[u8], offset: usize) -> u32 {
    u32::from(buffer[offset])
        | (u32::from(buffer[offset + 1]) << 8)
        | (u32::from(buffer[offset + 2]) << 16)
        | (u32::from(buffer[offset + 3]) << 24)
}

#[derive(Clone, Debug, Default)]
pub struct DdsFiles {
    dds_files: TigerHashMap<String, DdsInfo>,
}

impl DdsFiles {
    fn load_dds(entry: &FileEntry) -> Result<Option<DdsInfo>> {
        if metadata(entry.fullpath())?.len() == 0 {
            warn(ErrorKey::ImageFormat).msg("empty file").loc(entry).push();
            return Ok(None);
        }
        let mut f = File::open(entry.fullpath())?;
        let mut buffer = [0; DDS_HEADER_SIZE];
        f.read_exact(&mut buffer)?;
        if buffer.starts_with(b"\x89PNG") {
            let msg = "actually a PNG";
            let info =
                "this may be slower and lower quality because PNG format does not have mipmaps";
            tips(ErrorKey::ImageFormat).msg(msg).info(info).loc(entry).push();
            return Ok(None);
        }
        if !buffer.starts_with(b"DDS ") {
            err(ErrorKey::ImageFormat).msg("not a DDS file").loc(entry).push();
            return Ok(None);
        }
        Ok(Some(DdsInfo::new(entry.clone(), &buffer)))
    }

    fn handle_dds(&mut self, entry: &FileEntry, info: DdsInfo) {
        // Normalize separators so lookups by script path also work on Windows.
        self.dds_files.insert(entry.path().to_string_lossy().replace('\\', "/"), info);
    }

    pub fn validate(&self) {
        for item in self.dds_files.values() {
            item.validate();
        }
    }

    /// The game only supports B8G8R8A8 for some icons, such as regiment icons, and logs
    /// `Only B8G8R8A8_UNORM support so far` for any other pixel format (including DXT/BC).
    pub fn validate_bgra8(&self, path: &str, key: &Token) {
        if let Some(info) = self.dds_files.get(path)
            && !info.bgra8
            && !info.dx10
        {
            let msg = "icon DDS must be uncompressed B8G8R8A8";
            let info = "the game logs \"Only B8G8R8A8_UNORM support so far\" and shows no icon";
            err(ErrorKey::ImageFormat).msg(msg).info(info).loc(key).push();
        }
    }

    pub fn validate_frame(&self, key: &Token, width: u32, height: u32, frame: u32) {
        // Note: `frame` is 1-based
        if let Some(info) = self.dds_files.get(key.as_str()) {
            if info.height != height {
                let msg = format!("texture does not match frame height of {height}");
                warn(ErrorKey::ImageFormat).msg(msg).loc(key).push();
            } else if width == 0 || (info.width % width) != 0 {
                let msg = format!("texture is not a multiple of frame width {width}");
                warn(ErrorKey::ImageFormat).msg(msg).loc(key).push();
            } else if frame * width > info.width {
                let msg = format!("texture is not large enough for frame index {frame}");
                warn(ErrorKey::ImageFormat).msg(msg).loc(key).push();
            }
        }
    }
}

impl FileHandler<DdsInfo> for DdsFiles {
    fn subpath(&self) -> PathBuf {
        PathBuf::from("gfx")
    }

    fn load_file(&self, entry: &FileEntry, _parser: &ParserMemory) -> Option<DdsInfo> {
        if !entry.filename().to_string_lossy().ends_with(".dds") {
            return None;
        }

        match Self::load_dds(entry) {
            Ok(info) => info,
            Err(e) => {
                err(ErrorKey::ReadError)
                    .msg("could not read dds header")
                    .info(format!("{e:#}"))
                    .loc(entry)
                    .push();
                None
            }
        }
    }

    fn handle_file(&mut self, entry: &FileEntry, info: DdsInfo) {
        self.handle_dds(entry, info);
    }
}

#[derive(Clone, Debug)]
pub struct DdsInfo {
    entry: FileEntry,
    compressed: bool,
    /// Uncompressed 32-bit with masks R=0x00ff0000 G=0x0000ff00 B=0x000000ff A=0xff000000.
    bgra8: bool,
    /// A `DX10` extended header: the real format is in a second header that is not read here.
    dx10: bool,
    width: u32,
    height: u32,
}

impl DdsInfo {
    pub fn new(entry: FileEntry, header: &[u8]) -> Self {
        Self {
            entry,
            compressed: (from_le32(header, DDS_PIXELFORMAT_FLAGS_OFFSET) & 0x04) != 0,
            bgra8: (from_le32(header, DDS_PIXELFORMAT_FLAGS_OFFSET) & 0x45) == 0x41
                && from_le32(header, DDS_PIXELFORMAT_BITCOUNT_OFFSET) == 32
                && from_le32(header, DDS_PIXELFORMAT_MASKS_OFFSET) == 0x00ff_0000
                && from_le32(header, DDS_PIXELFORMAT_MASKS_OFFSET + 4) == 0x0000_ff00
                && from_le32(header, DDS_PIXELFORMAT_MASKS_OFFSET + 8) == 0x0000_00ff
                && from_le32(header, DDS_PIXELFORMAT_MASKS_OFFSET + 12) == 0xff00_0000,
            dx10: header.get(DDS_PIXELFORMAT_FOURCC_OFFSET..DDS_PIXELFORMAT_FOURCC_OFFSET + 4)
                == Some(b"DX10".as_slice()),
            width: from_le32(header, DDS_WIDTH_OFFSET),
            height: from_le32(header, DDS_HEIGHT_OFFSET),
        }
    }

    fn validate(&self) {
        if self.compressed && !(self.width.is_multiple_of(4) || self.height.is_multiple_of(4)) {
            let msg = "compressed DDS must have width and height divisible by 4";
            let info = format!(
                "DDS file is {}x{}, which can cause scaling problems and graphical artifacts",
                self.width, self.height
            );
            err(ErrorKey::ImageSize).msg(msg).info(info).loc(&self.entry).push();
        }
    }
}
