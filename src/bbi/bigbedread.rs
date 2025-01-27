use std::borrow::BorrowMut;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::vec::Vec;

use byteordered::ByteOrdered;
use thiserror::Error;

use crate::bbi::{BBIFile, BedEntry, ZoomRecord};
use crate::bbiread::{
    get_block_data, read_info, BBIFileInfo, BBIFileReadInfoError, BBIRead, BBIReadError, Block,
    ChromInfo, ZoomIntervalIter,
};
use crate::utils::reopen::{Reopen, ReopenableFile, SeekableRead};
use crate::{BBIReadInternal, ZoomIntervalError};

struct IntervalIter<I, R, B>
where
    I: Iterator<Item = Block> + Send,
    R: SeekableRead,
    B: BorrowMut<BigBedRead<R>>,
{
    r: std::marker::PhantomData<R>,
    bigbed: B,
    known_offset: u64,
    blocks: I,
    vals: Option<std::vec::IntoIter<BedEntry>>,
    expected_chrom: u32,
    start: u32,
    end: u32,
}

impl<I, R, B> Iterator for IntervalIter<I, R, B>
where
    I: Iterator<Item = Block> + Send,
    R: SeekableRead,
    B: BorrowMut<BigBedRead<R>>,
{
    type Item = Result<BedEntry, BBIReadError>;

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
                    // TODO: Could minimize this by chunking block reads
                    let current_block = self.blocks.next()?;
                    match get_block_entries(
                        self.bigbed.borrow_mut(),
                        current_block,
                        &mut self.known_offset,
                        self.expected_chrom,
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

/// Possible errors encountered when opening a bigBed file to read
#[derive(Error, Debug)]
pub enum BigBedReadOpenError {
    #[error("File is not a bigBed.")]
    NotABigBed,
    #[error("The chromosomes are invalid.")]
    InvalidChroms,
    #[error("An error occurred: {}", .0)]
    IoError(io::Error),
}

impl From<io::Error> for BigBedReadOpenError {
    fn from(error: io::Error) -> Self {
        BigBedReadOpenError::IoError(error)
    }
}

impl From<BBIFileReadInfoError> for BigBedReadOpenError {
    fn from(error: BBIFileReadInfoError) -> Self {
        match error {
            BBIFileReadInfoError::UnknownMagic => BigBedReadOpenError::NotABigBed,
            BBIFileReadInfoError::InvalidChroms => BigBedReadOpenError::InvalidChroms,
            BBIFileReadInfoError::IoError(e) => BigBedReadOpenError::IoError(e),
        }
    }
}

/// The struct used to read a bigBed file
pub struct BigBedRead<R> {
    info: BBIFileInfo,
    read: R,
}

impl<R: Reopen> Reopen for BigBedRead<R> {
    fn reopen(&self) -> io::Result<Self> {
        Ok(BigBedRead {
            info: self.info.clone(),
            read: self.read.reopen()?,
        })
    }
}

impl<R: SeekableRead> BBIRead for BigBedRead<R> {
    type Read = R;

    fn get_info(&self) -> &BBIFileInfo {
        &self.info
    }

    fn reader(&mut self) -> &mut R {
        &mut self.read
    }

    fn get_chroms(&self) -> Vec<ChromInfo> {
        self.info.chrom_info.clone()
    }
}

impl BigBedRead<ReopenableFile> {
    /// Opens a new `BigBedRead` from a given path as a file.
    pub fn open_file(path: &str) -> Result<Self, BigBedReadOpenError> {
        let reopen = ReopenableFile {
            path: path.to_string(),
            file: File::open(path)?,
        };
        let b = BigBedRead::open(reopen);
        if b.is_err() {
            eprintln!("Error when opening: {}", path);
        }
        b
    }
}

impl<R> BigBedRead<R>
where
    R: SeekableRead,
{
    /// Opens a new `BigBedRead` with for a given type that implements both `Read` and `Seek`
    pub fn open(mut read: R) -> Result<Self, BigBedReadOpenError> {
        let info = read_info(&mut read)?;
        match info.filetype {
            BBIFile::BigBed => {}
            _ => return Err(BigBedReadOpenError::NotABigBed),
        }

        Ok(BigBedRead { info, read })
    }

    /// Reads the autosql from this bigBed
    pub fn autosql(&mut self) -> Result<String, BBIReadError> {
        let auto_sql_offset = self.info.header.auto_sql_offset;
        let reader = self.reader();
        let mut reader = BufReader::new(reader);
        reader.seek(SeekFrom::Start(auto_sql_offset))?;
        let mut buffer = Vec::new();
        reader.read_until(b'\0', &mut buffer)?;
        buffer.pop();
        let autosql = String::from_utf8(buffer)
            .map_err(|_| BBIReadError::InvalidFile("Invalid autosql: not UTF-8".to_owned()))?;
        Ok(autosql)
    }

    /// For a given chromosome, start, and end, returns an `Iterator` of the
    /// intersecting `BedEntry`s. The resulting iterator takes a mutable reference
    /// of this `BigBedRead`.
    pub fn get_interval<'a>(
        &'a mut self,
        chrom_name: &str,
        start: u32,
        end: u32,
    ) -> Result<impl Iterator<Item = Result<BedEntry, BBIReadError>> + 'a, BBIReadError> {
        let blocks = self.get_overlapping_blocks(chrom_name, start, end)?;
        // TODO: this is only for asserting that the chrom is what we expect
        let chrom_ix = self
            .get_info()
            .chrom_info
            .iter()
            .find(|&x| x.name == chrom_name)
            .unwrap()
            .id;
        Ok(IntervalIter {
            r: std::marker::PhantomData,
            bigbed: self,
            known_offset: 0,
            blocks: blocks.into_iter(),
            vals: None,
            expected_chrom: chrom_ix,
            start,
            end,
        })
    }

    /// For a given chromosome, start, and end, returns an `Iterator` of the
    /// intersecting `BedEntry`s. The resulting iterator takes this `BigBedRead`
    /// by value.
    pub fn get_interval_move(
        mut self,
        chrom_name: &str,
        start: u32,
        end: u32,
    ) -> Result<impl Iterator<Item = Result<BedEntry, BBIReadError>>, BBIReadError> {
        let blocks = self.get_overlapping_blocks(chrom_name, start, end)?;
        // TODO: this is only for asserting that the chrom is what we expect
        let chrom_ix = self
            .get_info()
            .chrom_info
            .iter()
            .find(|&x| x.name == chrom_name)
            .unwrap()
            .id;
        Ok(IntervalIter {
            r: std::marker::PhantomData,
            bigbed: self,
            known_offset: 0,
            blocks: blocks.into_iter(),
            vals: None,
            expected_chrom: chrom_ix,
            start,
            end,
        })
    }

    /// For a given chromosome, start, and end, returns an `Iterator` of the
    /// intersecting `ZoomRecord`s.
    pub fn get_zoom_interval<'a>(
        &'a mut self,
        chrom_name: &str,
        start: u32,
        end: u32,
        reduction_level: u32,
    ) -> Result<impl Iterator<Item = Result<ZoomRecord, BBIReadError>> + 'a, ZoomIntervalError>
    {
        let chrom = self.info.chrom_id(chrom_name)?;
        let zoom_header = match self
            .info
            .zoom_headers
            .iter()
            .find(|h| h.reduction_level == reduction_level)
        {
            Some(h) => h,
            None => {
                return Err(ZoomIntervalError::ReductionLevelNotFound);
            }
        };

        let index_offset = zoom_header.index_offset;
        let blocks = self.search_cir_tree(index_offset, chrom_name, start, end)?;
        Ok(ZoomIntervalIter::new(
            self,
            blocks.into_iter(),
            chrom,
            start,
            end,
        ))
    }
}

// TODO: remove expected_chrom
fn get_block_entries<R: SeekableRead>(
    bigbed: &mut BigBedRead<R>,
    block: Block,
    known_offset: &mut u64,
    expected_chrom: u32,
    start: u32,
    end: u32,
) -> Result<std::vec::IntoIter<BedEntry>, BBIReadError> {
    let block_data_mut = get_block_data(bigbed, &block, *known_offset)?;
    let mut block_data_mut = ByteOrdered::runtime(block_data_mut, bigbed.info.header.endianness);
    let mut entries: Vec<BedEntry> = Vec::new();

    let mut read_entry = || -> Result<BedEntry, BBIReadError> {
        let chrom_id = block_data_mut.read_u32()?;
        let chrom_start = block_data_mut.read_u32()?;
        let chrom_end = block_data_mut.read_u32()?;
        if chrom_start == 0 && chrom_end == 0 {
            return Err(BBIReadError::InvalidFile(
                "Chrom start and end both equal 0.".to_owned(),
            ));
        }
        // FIXME: should this just return empty?
        assert_eq!(
            chrom_id, expected_chrom,
            "BUG: bigBed had multiple chroms in a section"
        );
        let s: Vec<u8> = block_data_mut
            .by_ref()
            .bytes()
            .take_while(|c| {
                if let Ok(c) = c {
                    return *c != b'\0';
                }
                false
            })
            .collect::<Result<Vec<u8>, _>>()?;
        let rest = String::from_utf8(s).unwrap();
        Ok(BedEntry {
            start: chrom_start,
            end: chrom_end,
            rest,
        })
    };
    while let Ok(entry) = read_entry() {
        if entry.end >= start && entry.start <= end {
            entries.push(entry);
        }
    }

    *known_offset = block.offset + block.size;
    Ok(entries.into_iter())
}
