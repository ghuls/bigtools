use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::vec::Vec;

use byteordered::Endianness;
use bytes::{Buf, BytesMut};
use thiserror::Error;

use crate::bbi::{
    BBIFile, Summary, ZoomHeader, ZoomRecord, BIGBED_MAGIC, BIGWIG_MAGIC, CHROM_TREE_MAGIC,
    CIR_TREE_MAGIC,
};
use crate::bed::bedparser::BedValueError;
use crate::utils::reopen::SeekableRead;

#[derive(Copy, Clone, Debug)]
pub(crate) struct Block {
    pub offset: u64,
    pub size: u64,
}

/// Header info for a bbi file
///
/// Note that info on internal properties like file offsets are not public.
/// Reading data is available through higher-level functions.
#[derive(Copy, Clone, Debug)]
pub struct BBIHeader {
    pub endianness: Endianness,
    pub version: u16,
    pub field_count: u16,
    pub defined_field_count: u16,

    pub(crate) zoom_levels: u16,
    pub(crate) chromosome_tree_offset: u64,
    pub(crate) full_data_offset: u64,
    pub(crate) full_index_offset: u64,
    pub(crate) auto_sql_offset: u64,
    pub(crate) total_summary_offset: u64,
    pub(crate) uncompress_buf_size: u32,
}

/// Information on a chromosome in a bbi file
#[derive(Clone, Debug)]
pub struct ChromInfo {
    pub name: String,
    pub length: u32,
    pub(crate) id: u32,
}

impl PartialEq for ChromInfo {
    fn eq(&self, other: &ChromInfo) -> bool {
        self.name == other.name
    }
}

/// Info on a bbi file
#[derive(Clone, Debug)]
pub struct BBIFileInfo {
    /// The type of the bbi file - either a bigBed or a bigWig
    pub filetype: BBIFile,
    /// Header info
    pub header: BBIHeader,
    /// Info on zooms in the bbi file
    pub zoom_headers: Vec<ZoomHeader>,
    /// The chromosome info the bbi file is based on
    pub chrom_info: Vec<ChromInfo>,
}

pub(crate) struct ChromIdNotFound(pub(crate) String);

impl From<ChromIdNotFound> for BBIReadError {
    fn from(e: ChromIdNotFound) -> Self {
        BBIReadError::InvalidChromosome(e.0)
    }
}

impl BBIFileInfo {
    pub(crate) fn chrom_id(&self, chrom_name: &str) -> Result<u32, ChromIdNotFound> {
        let chrom_info = &self.chrom_info;
        let chrom = chrom_info.iter().find(|&x| x.name == chrom_name);
        match chrom {
            Some(c) => Ok(c.id),
            None => Err(ChromIdNotFound(chrom_name.to_owned())),
        }
    }
}

#[derive(Error, Debug)]
pub(crate) enum BBIFileReadInfoError {
    #[error("Invalid magic (likely not a BigWig or BigBed file)")]
    UnknownMagic,
    #[error("Invalid chromosomes section")]
    InvalidChroms,
    #[error("Error occurred: {}", .0)]
    IoError(#[from] io::Error),
}

#[derive(Error, Debug)]
pub(crate) enum CirTreeSearchError {
    #[error("The passed chromosome ({}) was incorrect.", .0)]
    InvalidChromosome(String),
    #[error("Invalid magic (likely a bug).")]
    UnknownMagic,
    #[error("Error occurred: {}", .0)]
    IoError(#[from] io::Error),
}

/// Possible errors encountered when reading a bbi file
#[derive(Error, Debug)]
pub enum BBIReadError {
    #[error("The passed chromosome ({}) was incorrect.", .0)]
    InvalidChromosome(String),
    #[error("Invalid magic (likely a bug).")]
    UnknownMagic,
    #[error("The file was invalid: {}", .0)]
    InvalidFile(String),
    #[error("Error parsing bed-like data.")]
    BedValueError(#[from] BedValueError),
    #[error("Error occurred: {}", .0)]
    IoError(#[from] io::Error),
}

impl From<CirTreeSearchError> for BBIReadError {
    fn from(value: CirTreeSearchError) -> Self {
        match value {
            CirTreeSearchError::InvalidChromosome(chrom) => BBIReadError::InvalidChromosome(chrom),
            CirTreeSearchError::UnknownMagic => BBIReadError::UnknownMagic,
            CirTreeSearchError::IoError(e) => BBIReadError::IoError(e),
        }
    }
}

/// Potential errors found when trying to read data from a zoom level
#[derive(Error, Debug)]
pub enum ZoomIntervalError {
    #[error("The passed reduction level was not found")]
    ReductionLevelNotFound,
    #[error("{}", .0)]
    BBIReadError(BBIReadError),
}

impl From<ChromIdNotFound> for ZoomIntervalError {
    fn from(e: ChromIdNotFound) -> Self {
        ZoomIntervalError::BBIReadError(BBIReadError::InvalidChromosome(e.0))
    }
}

impl From<CirTreeSearchError> for ZoomIntervalError {
    fn from(e: CirTreeSearchError) -> Self {
        ZoomIntervalError::BBIReadError(e.into())
    }
}

pub(crate) trait BBIReadInternal: BBIRead {
    /// This assumes the file is at the cir tree start
    fn search_cir_tree(
        &mut self,
        at: u64,
        chrom_name: &str,
        start: u32,
        end: u32,
    ) -> Result<Vec<Block>, CirTreeSearchError> {
        // TODO: Move anything relying on self out to separate method
        let chrom_ix = {
            let chrom_info = &self.get_info().chrom_info;
            let chrom = chrom_info.iter().find(|&x| x.name == chrom_name);
            match chrom {
                Some(c) => c.id,
                None => {
                    return Err(CirTreeSearchError::InvalidChromosome(
                        chrom_name.to_string(),
                    ));
                }
            }
        };

        let endianness = self.get_info().header.endianness;
        let mut file = self.reader();
        file.seek(SeekFrom::Start(at))?;
        let mut header_data = BytesMut::zeroed(48);
        file.read_exact(&mut header_data)?;

        match endianness {
            Endianness::Big => {
                let magic = header_data.get_u32();
                if magic != CIR_TREE_MAGIC {
                    return Err(CirTreeSearchError::UnknownMagic);
                }

                let _blocksize = header_data.get_u32();
                let _item_count = header_data.get_u64();
                let _start_chrom_idx = header_data.get_u32();
                let _start_base = header_data.get_u32();
                let _end_chrom_idx = header_data.get_u32();
                let _end_base = header_data.get_u32();
                let _end_file_offset = header_data.get_u64();
                let _item_per_slot = header_data.get_u32();
                let _reserved = header_data.get_u32();
            }
            Endianness::Little => {
                let magic = header_data.get_u32_le();
                if magic != CIR_TREE_MAGIC {
                    return Err(CirTreeSearchError::UnknownMagic);
                }

                let _blocksize = header_data.get_u32_le();
                let _item_count = header_data.get_u64_le();
                let _start_chrom_idx = header_data.get_u32_le();
                let _start_base = header_data.get_u32_le();
                let _end_chrom_idx = header_data.get_u32_le();
                let _end_base = header_data.get_u32_le();
                let _end_file_offset = header_data.get_u64_le();
                let _item_per_slot = header_data.get_u32_le();
                let _reserved = header_data.get_u32_le();
            }
        };

        // TODO: could do some optimization here to check if our interval overlaps with any data

        let mut blocks: Vec<Block> = vec![];
        search_overlapping_blocks(&mut file, endianness, chrom_ix, start, end, &mut blocks)?;
        Ok(blocks)
    }

    fn get_overlapping_blocks(
        &mut self,
        chrom_name: &str,
        start: u32,
        end: u32,
    ) -> Result<Vec<Block>, CirTreeSearchError> {
        let full_index_offset = self.get_info().header.full_index_offset;
        self.search_cir_tree(full_index_offset, chrom_name, start, end)
    }
}

impl<T: BBIRead> BBIReadInternal for T {}

/// Generic methods for reading a bbi file
pub trait BBIRead {
    type Read: SeekableRead;

    /// Get basic info about the bbi file
    fn get_info(&self) -> &BBIFileInfo;

    /// Gets a reader to the underlying file
    fn reader(&mut self) -> &mut Self::Read;

    fn get_chroms(&self) -> Vec<ChromInfo>;
}

pub(crate) fn read_info<R: SeekableRead>(
    mut file: &mut R,
) -> Result<BBIFileInfo, BBIFileReadInfoError> {
    let mut header_data = BytesMut::zeroed(64);
    file.read_exact(&mut header_data)?;

    let magic = header_data.get_u32();
    let (filetype, endianness) = match magic {
        _ if magic == BIGWIG_MAGIC.to_le() => (BBIFile::BigWig, Endianness::Big),
        _ if magic == BIGWIG_MAGIC.to_be() => (BBIFile::BigWig, Endianness::Little),
        _ if magic == BIGBED_MAGIC.to_le() => (BBIFile::BigBed, Endianness::Big),
        _ if magic == BIGBED_MAGIC.to_be() => (BBIFile::BigBed, Endianness::Little),
        _ => return Err(BBIFileReadInfoError::UnknownMagic),
    };

    let (
        version,
        zoom_levels,
        chromosome_tree_offset,
        full_data_offset,
        full_index_offset,
        field_count,
        defined_field_count,
        auto_sql_offset,
        total_summary_offset,
        uncompress_buf_size,
    ) = match endianness {
        Endianness::Big => {
            let version = header_data.get_u16();
            let zoom_levels = header_data.get_u16();
            let chromosome_tree_offset = header_data.get_u64();
            let full_data_offset = header_data.get_u64();
            let full_index_offset = header_data.get_u64();
            let field_count = header_data.get_u16();
            let defined_field_count = header_data.get_u16();
            let auto_sql_offset = header_data.get_u64();
            let total_summary_offset = header_data.get_u64();
            let uncompress_buf_size = header_data.get_u32();
            let _reserved = header_data.get_u64();

            (
                version,
                zoom_levels,
                chromosome_tree_offset,
                full_data_offset,
                full_index_offset,
                field_count,
                defined_field_count,
                auto_sql_offset,
                total_summary_offset,
                uncompress_buf_size,
            )
        }
        Endianness::Little => {
            let version = header_data.get_u16_le();
            let zoom_levels = header_data.get_u16_le();
            let chromosome_tree_offset = header_data.get_u64_le();
            let full_data_offset = header_data.get_u64_le();
            let full_index_offset = header_data.get_u64_le();
            let field_count = header_data.get_u16_le();
            let defined_field_count = header_data.get_u16_le();
            let auto_sql_offset = header_data.get_u64_le();
            let total_summary_offset = header_data.get_u64_le();
            let uncompress_buf_size = header_data.get_u32_le();
            let _reserved = header_data.get_u64_le();

            (
                version,
                zoom_levels,
                chromosome_tree_offset,
                full_data_offset,
                full_index_offset,
                field_count,
                defined_field_count,
                auto_sql_offset,
                total_summary_offset,
                uncompress_buf_size,
            )
        }
    };

    let header = BBIHeader {
        endianness,
        version,
        zoom_levels,
        chromosome_tree_offset,
        full_data_offset,
        full_index_offset,
        field_count,
        defined_field_count,
        auto_sql_offset,
        total_summary_offset,
        uncompress_buf_size,
    };

    let zoom_headers = read_zoom_headers(&mut file, &header)?;

    // TODO: could instead store this as an Option and only read when needed
    file.seek(SeekFrom::Start(header.chromosome_tree_offset))?;

    let mut header_data = BytesMut::zeroed(32);
    file.read_exact(&mut header_data)?;

    let (key_size, val_size, item_count) = match endianness {
        Endianness::Big => {
            let magic = header_data.get_u32();
            if magic != CHROM_TREE_MAGIC {
                return Err(BBIFileReadInfoError::InvalidChroms);
            }

            let _block_size = header_data.get_u32();
            let key_size = header_data.get_u32();
            let val_size = header_data.get_u32();
            let item_count = header_data.get_u64();
            let _reserved = header_data.get_u64();

            (key_size, val_size, item_count)
        }
        Endianness::Little => {
            let magic = header_data.get_u32_le();
            if magic != CHROM_TREE_MAGIC {
                return Err(BBIFileReadInfoError::InvalidChroms);
            }

            let _block_size = header_data.get_u32_le();
            let key_size = header_data.get_u32_le();
            let val_size = header_data.get_u32_le();
            let item_count = header_data.get_u64_le();
            let _reserved = header_data.get_u64_le();

            (key_size, val_size, item_count)
        }
    };

    assert_eq!(val_size, 8u32);

    let mut chrom_info = Vec::with_capacity(item_count as usize);
    read_chrom_tree_block(&mut file, endianness, &mut chrom_info, key_size)
        .map_err(|_| BBIFileReadInfoError::InvalidChroms)?;
    chrom_info.sort_by(|c1, c2| c1.name.cmp(&c2.name));

    let info = BBIFileInfo {
        filetype,
        header,
        zoom_headers,
        chrom_info,
    };

    Ok(info)
}

fn read_zoom_headers<R: SeekableRead>(
    file: &mut R,
    header: &BBIHeader,
) -> io::Result<Vec<ZoomHeader>> {
    let endianness = header.endianness;
    let mut header_data = BytesMut::zeroed((header.zoom_levels as usize) * 24);
    file.read_exact(&mut header_data)?;

    let mut zoom_headers = vec![];
    match endianness {
        Endianness::Big => {
            for _ in 0..header.zoom_levels {
                let reduction_level = header_data.get_u32();
                let _reserved = header_data.get_u32();
                let data_offset = header_data.get_u64();
                let index_offset = header_data.get_u64();

                zoom_headers.push(ZoomHeader {
                    reduction_level,
                    data_offset,
                    index_offset,
                });
            }
        }
        Endianness::Little => {
            for _ in 0..header.zoom_levels {
                let reduction_level = header_data.get_u32_le();
                let _reserved = header_data.get_u32_le();
                let data_offset = header_data.get_u64_le();
                let index_offset = header_data.get_u64_le();

                zoom_headers.push(ZoomHeader {
                    reduction_level,
                    data_offset,
                    index_offset,
                });
            }
        }
    };

    Ok(zoom_headers)
}

#[derive(Error, Debug)]
enum ChromTreeBlockReadError {
    #[error("{}", .0)]
    InvalidFile(String),
    #[error("Error occurred: {}", .0)]
    IoError(#[from] io::Error),
}

fn read_chrom_tree_block<R: SeekableRead>(
    f: &mut R,
    endianness: Endianness,
    chroms: &mut Vec<ChromInfo>,
    key_size: u32,
) -> Result<(), ChromTreeBlockReadError> {
    let mut header_data = BytesMut::zeroed(4);
    f.read_exact(&mut header_data)?;

    let isleaf = header_data.get_u8();
    let _reserved = header_data.get_u8();
    let count = match endianness {
        Endianness::Big => header_data.get_u16(),
        Endianness::Little => header_data.get_u16_le(),
    };

    if isleaf == 1 {
        let mut bytes = BytesMut::zeroed((key_size as usize + 8) * (count as usize));
        f.read_exact(&mut bytes)?;

        for _ in 0..count {
            let key_string = match std::str::from_utf8(&bytes.as_ref()[0..(key_size as usize)]) {
                Ok(s) => s.trim_matches(char::from(0)).to_owned(),
                Err(_) => {
                    return Err(ChromTreeBlockReadError::InvalidFile(
                        "Invalid file format: Invalid utf-8 string.".to_owned(),
                    ))
                }
            };
            bytes.advance(key_size as usize);

            let (chrom_id, chrom_size) = match endianness {
                Endianness::Big => (bytes.get_u32(), bytes.get_u32()),
                Endianness::Little => (bytes.get_u32_le(), bytes.get_u32_le()),
            };
            chroms.push(ChromInfo {
                name: key_string,
                id: chrom_id,
                length: chrom_size,
            });
        }
    } else {
        // First, go through and get child blocks
        let mut children: Vec<u64> = vec![];
        children.reserve_exact(count as usize);

        let mut bytes = BytesMut::zeroed((key_size as usize + 8) * (count as usize));
        f.read_exact(&mut bytes)?;

        for _ in 0..count {
            // We don't need this, but have to read it
            bytes.advance(key_size as usize);

            // TODO: could add specific find here by comparing key string
            let child_offset = match endianness {
                Endianness::Big => bytes.get_u64(),
                Endianness::Little => bytes.get_u64_le(),
            };
            children.push(child_offset);
        }
        // Then go through each child block
        for child in children {
            f.seek(SeekFrom::Start(child))?;
            read_chrom_tree_block(f, endianness, chroms, key_size)?;
        }
    }
    Ok(())
}

#[inline]
fn compare_position(chrom1: u32, chrom1_base: u32, chrom2: u32, chrom2_base: u32) -> i8 {
    if chrom1 < chrom2 {
        -1
    } else if chrom1 > chrom2 {
        1
    } else if chrom1_base < chrom2_base {
        -1
    } else if chrom1_base > chrom2_base {
        1
    } else {
        0
    }
}

#[inline]
fn overlaps(
    chromq: u32,
    chromq_start: u32,
    chromq_end: u32,
    chromb1: u32,
    chromb1_start: u32,
    chromb2: u32,
    chromb2_end: u32,
) -> bool {
    compare_position(chromq, chromq_start, chromb2, chromb2_end) <= 0
        && compare_position(chromq, chromq_end, chromb1, chromb1_start) >= 0
}

pub(crate) fn search_overlapping_blocks<R: SeekableRead>(
    file: &mut R,
    endianness: Endianness,
    chrom_ix: u32,
    start: u32,
    end: u32,
    blocks: &mut Vec<Block>,
) -> io::Result<()> {
    let mut header_data = BytesMut::zeroed(4);
    file.read_exact(&mut header_data)?;

    let isleaf: u8 = header_data.get_u8();
    assert!(isleaf == 1 || isleaf == 0, "Unexpected isleaf: {}", isleaf);
    let _reserved = header_data.get_u8();

    let count = match endianness {
        Endianness::Big => header_data.get_u16(),
        Endianness::Little => header_data.get_u16_le(),
    };

    if isleaf == 1 {
        let mut bytes = vec![0u8; (count as usize) * 32];
        file.read_exact(&mut bytes)?;

        for i in 0..(count as usize) {
            let istart = i * 32;
            let bytes: &[u8; 32] = &bytes[istart..istart + 32].try_into().unwrap();
            let (start_chrom_ix, start_base, end_chrom_ix, end_base, data_offset, data_size) =
                match endianness {
                    Endianness::Big => {
                        let start_chrom_ix =
                            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                        let start_base =
                            u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                        let end_chrom_ix =
                            u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
                        let end_base =
                            u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
                        let data_offset = u64::from_be_bytes([
                            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21],
                            bytes[22], bytes[23],
                        ]);
                        let data_size = u64::from_be_bytes([
                            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29],
                            bytes[30], bytes[31],
                        ]);

                        (
                            start_chrom_ix,
                            start_base,
                            end_chrom_ix,
                            end_base,
                            data_offset,
                            data_size,
                        )
                    }
                    Endianness::Little => {
                        let start_chrom_ix =
                            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                        let start_base =
                            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                        let end_chrom_ix =
                            u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
                        let end_base =
                            u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
                        let data_offset = u64::from_le_bytes([
                            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21],
                            bytes[22], bytes[23],
                        ]);
                        let data_size = u64::from_le_bytes([
                            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29],
                            bytes[30], bytes[31],
                        ]);

                        (
                            start_chrom_ix,
                            start_base,
                            end_chrom_ix,
                            end_base,
                            data_offset,
                            data_size,
                        )
                    }
                };
            let block_overlaps = overlaps(
                chrom_ix,
                start,
                end,
                start_chrom_ix,
                start_base,
                end_chrom_ix,
                end_base,
            );
            if block_overlaps {
                blocks.push(Block {
                    offset: data_offset,
                    size: data_size,
                });
            }
        }
    } else {
        let mut bytes = vec![0u8; (count as usize) * 32];
        file.read_exact(&mut bytes)?;

        let mut childblocks: Vec<u64> = vec![];
        for i in 0..(count as usize) {
            let istart = i * 24;
            let bytes: &[u8; 24] = &bytes[istart..istart + 24].try_into().unwrap();
            let (start_chrom_ix, start_base, end_chrom_ix, end_base, data_offset) = match endianness
            {
                Endianness::Big => {
                    let start_chrom_ix =
                        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    let start_base = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                    let end_chrom_ix =
                        u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
                    let end_base = u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
                    let data_offset = u64::from_be_bytes([
                        bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21],
                        bytes[22], bytes[23],
                    ]);

                    (
                        start_chrom_ix,
                        start_base,
                        end_chrom_ix,
                        end_base,
                        data_offset,
                    )
                }
                Endianness::Little => {
                    let start_chrom_ix =
                        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    let start_base = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
                    let end_chrom_ix =
                        u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
                    let end_base = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
                    let data_offset = u64::from_le_bytes([
                        bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21],
                        bytes[22], bytes[23],
                    ]);

                    (
                        start_chrom_ix,
                        start_base,
                        end_chrom_ix,
                        end_base,
                        data_offset,
                    )
                }
            };
            let block_overlaps = overlaps(
                chrom_ix,
                start,
                end,
                start_chrom_ix,
                start_base,
                end_chrom_ix,
                end_base,
            );
            if block_overlaps {
                childblocks.push(data_offset);
            }
        }
        for childblock in childblocks {
            file.seek(SeekFrom::Start(childblock))?;
            search_overlapping_blocks(file, endianness, chrom_ix, start, end, blocks)?;
        }
    }
    Ok(())
}

/// Gets the data (uncompressed, if applicable) from a given block
pub(crate) fn get_block_data<B: BBIRead>(
    bbifile: &mut B,
    block: &Block,
    known_offset: u64,
) -> io::Result<Cursor<Vec<u8>>> {
    use libdeflater::Decompressor;

    let uncompress_buf_size = bbifile.get_info().header.uncompress_buf_size as usize;
    let file = bbifile.reader();

    // TODO: Could minimize this by chunking block reads
    // FIXME: this relies on the current state of "store a BufReader as a reader"
    if known_offset != block.offset {
        file.seek(SeekFrom::Start(block.offset))?;
    }

    let mut raw_data = vec![0u8; block.size as usize];
    file.read_exact(&mut raw_data)?;
    let block_data: Vec<u8> = if uncompress_buf_size > 0 {
        let mut decompressor = Decompressor::new();
        let mut outbuf = vec![0; uncompress_buf_size];
        let decompressed = decompressor
            .zlib_decompress(&raw_data, &mut outbuf)
            .unwrap();
        outbuf.truncate(decompressed);
        outbuf
    } else {
        raw_data
    };

    Ok(Cursor::new(block_data))
}

pub(crate) fn get_zoom_block_values<B: BBIRead>(
    bbifile: &mut B,
    block: Block,
    known_offset: &mut u64,
    chrom: u32,
    start: u32,
    end: u32,
) -> Result<Box<dyn Iterator<Item = ZoomRecord> + Send>, BBIReadError> {
    let mut data_mut = get_block_data(bbifile, &block, *known_offset)?;
    let len = data_mut.get_mut().len();
    assert_eq!(len % (4 * 8), 0);
    let itemcount = len / (4 * 8);
    let mut records = Vec::with_capacity(itemcount);

    let endianness = bbifile.get_info().header.endianness;

    let mut bytes = BytesMut::zeroed(itemcount * (4 * 8));
    data_mut.read_exact(&mut bytes)?;
    match endianness {
        Endianness::Big => {
            for _ in 0..itemcount {
                let chrom_id = bytes.get_u32();
                let chrom_start = bytes.get_u32();
                let chrom_end = bytes.get_u32();
                let bases_covered = u64::from(bytes.get_u32());
                let min_val = f64::from(bytes.get_f32());
                let max_val = f64::from(bytes.get_f32());
                let sum = f64::from(bytes.get_f32());
                let sum_squares = f64::from(bytes.get_f32());
                if chrom_id == chrom && chrom_end >= start && chrom_start <= end {
                    records.push(ZoomRecord {
                        chrom: chrom_id,
                        start: chrom_start,
                        end: chrom_end,
                        summary: Summary {
                            total_items: 0,
                            bases_covered,
                            min_val,
                            max_val,
                            sum,
                            sum_squares,
                        },
                    });
                }
            }
        }
        Endianness::Little => {
            for _ in 0..itemcount {
                let chrom_id = bytes.get_u32_le();
                let chrom_start = bytes.get_u32_le();
                let chrom_end = bytes.get_u32_le();
                let bases_covered = u64::from(bytes.get_u32_le());
                let min_val = f64::from(bytes.get_f32_le());
                let max_val = f64::from(bytes.get_f32_le());
                let sum = f64::from(bytes.get_f32_le());
                let sum_squares = f64::from(bytes.get_f32_le());
                if chrom_id == chrom && chrom_end >= start && chrom_start <= end {
                    records.push(ZoomRecord {
                        chrom: chrom_id,
                        start: chrom_start,
                        end: chrom_end,
                        summary: Summary {
                            total_items: 0,
                            bases_covered,
                            min_val,
                            max_val,
                            sum,
                            sum_squares,
                        },
                    });
                }
            }
        }
    }

    *known_offset = block.offset + block.size;
    Ok(Box::new(records.into_iter()))
}

pub(crate) struct ZoomIntervalIter<'a, I, B>
where
    I: Iterator<Item = Block> + Send,
    B: BBIRead,
{
    bbifile: &'a mut B,
    known_offset: u64,
    blocks: I,
    vals: Option<Box<dyn Iterator<Item = ZoomRecord> + Send + 'a>>,
    chrom: u32,
    start: u32,
    end: u32,
}

impl<'a, I, B> ZoomIntervalIter<'a, I, B>
where
    I: Iterator<Item = Block> + Send,
    B: BBIRead,
{
    pub fn new(bbifile: &'a mut B, blocks: I, chrom: u32, start: u32, end: u32) -> Self {
        ZoomIntervalIter {
            bbifile,
            known_offset: 0,
            blocks,
            vals: None,
            chrom,
            start,
            end,
        }
    }
}

impl<'a, I, B> Iterator for ZoomIntervalIter<'a, I, B>
where
    I: Iterator<Item = Block> + Send,
    B: BBIRead,
{
    type Item = Result<ZoomRecord, BBIReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match &mut self.vals {
                Some(vals) => match vals.next() {
                    Some(v) => {
                        return Some(Ok(v));
                    }
                    None => {
                        self.vals = None;
                    }
                },
                None => {
                    let current_block = self.blocks.next()?;
                    match get_zoom_block_values(
                        self.bbifile,
                        current_block,
                        &mut self.known_offset,
                        self.chrom,
                        self.start,
                        self.end,
                    ) {
                        Ok(vals) => {
                            self.vals = Some(vals);
                        }
                        Err(e) => {
                            return Some(Err(e));
                        }
                    }
                }
            }
        }
    }
}
